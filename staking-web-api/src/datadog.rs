use datadog_apm::{ErrorInfo, HttpInfo, Span, SqlInfo, Trace};
use rocket::{
    fairing::{Fairing, Info, Kind},
    http::Status,
    Data, Request, Response, State,
};
use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};
use tracing::warn;

pub struct RequestTimer;

#[derive(Clone)]
struct TimerStart(Option<SystemTime>);

#[rocket::async_trait]
impl Fairing for RequestTimer {
    fn info(&self) -> Info {
        Info {
            name: "Datadog trace",
            kind: Kind::Request | Kind::Response,
        }
    }

    async fn on_request(&self, request: &mut Request<'_>, _: &mut Data<'_>) {
        let staking_config = request
            .guard::<&State<crate::pool::StakingConfig>>()
            .await
            .unwrap();
        let url = request.uri().to_string();
        if !staking_config.enable_datadog || url.eq("/") {
            return;
        }
        request.local_cache(|| TimerStart(Some(SystemTime::now())));
    }

    async fn on_response<'r>(&self, request: &'r Request<'_>, response: &mut Response<'r>) {
        let staking_config = request
            .guard::<&State<crate::pool::StakingConfig>>()
            .await
            .unwrap();

        let url = request.uri().to_string();
        if !staking_config.enable_datadog || response.status() == Status::NotFound || url.eq("/") {
            return;
        }

        let datadog_client = request
            .guard::<&State<datadog_apm::Client>>()
            .await
            .unwrap()
            .inner()
            .clone();

        let request_type = request.method().as_str();
        let path = request.uri().path().as_str();
        let status_code = response.status().code;
        let system_time_start = request.local_cache(|| TimerStart(None));

        let error = if status_code == 200 {
            None
        } else {
            let msg = if status_code == 500 {
                "Internal error".to_owned()
            } else {
                "Bad input error".to_owned()
            };
            Some(ErrorInfo {
                r#type: "unknown".to_owned(),
                msg,
                stack: "".to_owned(),
            })
        };

        send_trace(
            datadog_client,
            "request".to_owned(),
            request_type,
            url,
            path,
            status_code,
            system_time_start.0.unwrap(),
            "web".to_owned(),
            error,
            None,
        )
        .await;
    }
}

async fn send_trace(
    datadog_client: datadog_apm::Client,
    name: String,
    request_type: &str,
    url: String,
    path: &str,
    status_code: u16,
    system_time_start: SystemTime,
    resource_type: String,
    error: Option<ErrorInfo>,
    sql: Option<SqlInfo>,
) {
    let duration = match SystemTime::now().duration_since(system_time_start) {
        Ok(d) => d,
        Err(error) => {
            warn!("Failed calculating duration: {}", error);
            Duration::from_millis(0)
        }
    };

    let span = Span {
        id: 1,
        parent_id: None,
        name,
        resource: request_type.to_owned() + " " + path,
        r#type: resource_type,
        start: system_time_start,
        duration,
        http: Some(HttpInfo {
            url,
            method: request_type.to_owned(),
            status_code: status_code.to_string(),
        }),
        error,
        sql,
        tags: HashMap::new(),
    };

    let trace = Trace {
        id: 1,
        priority: 1,
        spans: vec![span],
    };

    datadog_client.send_trace(trace);
}
