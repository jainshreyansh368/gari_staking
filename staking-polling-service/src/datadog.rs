use datadog_apm::{ErrorInfo, HttpInfo, Span, SqlInfo, Trace};
use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};
use tracing::warn;

pub async fn send_trace(
    datadog_client: Option<&datadog_apm::Client>,
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
    if datadog_client.is_none() {
        return;
    }
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
        // web, db, cache, custom
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

    datadog_client.unwrap().clone().send_trace(trace);
}
