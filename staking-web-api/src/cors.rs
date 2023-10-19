use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::Header;
use rocket::http::Status;
use rocket::{Request, Response};
use std::collections::HashSet;

pub struct OriginHeader {
    pub allowed_domains: HashSet<String>,
}

#[rocket::async_trait]
impl Fairing for OriginHeader {
    fn info(&self) -> Info {
        Info {
            name: "CORS Policy",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, request: &'r Request<'_>, response: &mut Response<'r>) {
        if response.status() == Status::NotFound {
            return;
        }

        match request.headers().get_one("Origin") {
            None => {}
            Some(origin) => {
                if self.allowed_domains.contains(origin) {
                    let origin_header = Header::new("Access-Control-Allow-Origin", origin);
                    response.set_header(origin_header);
                    let methods_header = Header::new("Access-Control-Allow-Methods", "GET");
                    response.set_header(methods_header);
                }
            }
        };
    }
}
