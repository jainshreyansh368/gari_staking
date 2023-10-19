use rocket::fairing::AdHoc;

pub mod transaction;

pub fn mount() -> AdHoc {
    AdHoc::on_ignite("Attaching Routes", |rocket| async {
        rocket.mount(
            "/",
            routes![transaction::encode, transaction::send, transaction::sign],
        )
    })
}
