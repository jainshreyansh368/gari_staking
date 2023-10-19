use rocket::{
    fairing::{Fairing, Info, Kind},
    http::{uri::Origin, Method},
    Data, Request, State,
};
use tracing::info;

pub struct MaintenanceMode;

#[rocket::async_trait]
impl Fairing for MaintenanceMode {
    fn info(&self) -> Info {
        Info {
            name: "Maintenance Mode",
            kind: Kind::Request,
        }
    }

    async fn on_request(&self, request: &mut Request<'_>, _: &mut Data<'_>) {
        let staking_config = request
            .guard::<&State<crate::pool::StakingConfig>>()
            .await
            .unwrap();
        let url = request.uri().to_string();
        if staking_config.enable_maintenance && url.ne("/") {
            let uri = Origin::parse("/maintenance_mode").unwrap();
            request.set_uri(uri);
            request.set_method(Method::Get);
            info!("URI: {}", request.uri());
            return;
        }
    }
}
