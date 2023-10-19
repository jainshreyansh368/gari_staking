use rocket::fairing::AdHoc;

pub mod history;
pub mod leaderboard;
pub mod transaction;
pub mod user_and_stake;
pub mod web3_auth;

pub fn mount() -> AdHoc {
    AdHoc::on_ignite("Attaching Routes", |rocket| async {
        rocket.mount(
            "/",
            routes![
                history::get_user_history,
                history::user_transaction_details,
                leaderboard::get,
                transaction::encode,
                transaction::send,
                user_and_stake::get_details,
                web3_auth::login,
                web3_auth::verify
            ],
        )
    })
}
