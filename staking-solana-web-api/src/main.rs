mod dto;
mod helper;
mod instruction;
mod routes;
mod rpc_wrapper;
mod staking;

use base64::{engine::general_purpose, Engine as _};
use chrono::prelude::*;
use log::warn;
use rocket::{serde::json::Json, Config, State};
use solana_client::rpc_client::RpcClient;
use solana_program::clock;
use solana_sdk::{message::Message, pubkey::Pubkey, transaction::Transaction};
use std::collections::{HashMap, HashSet};

#[macro_use]
extern crate rocket;
extern crate chrono;
extern crate log;
extern crate pretty_env_logger;

#[get("/get_transactions_and_account_info?<limit>&<before>&<until>")]
async fn get_transactions_and_account_info(
    rpc_client: &State<RpcClient>,
    staking_config: &State<staking::StakingConfig>,
    limit: Option<usize>,
    before: Option<&str>,
    until: Option<&str>,
) -> Json<(
    HashMap<String, rpc_wrapper::StakingUserDataAccount>,
    usize,
    String,
    Vec<String>,
)> {
    let signatures = rpc_wrapper::get_signatures(
        rpc_client.inner(),
        &staking_config.staking_program_address,
        limit,
        before,
        until,
    );
    let mut total_signatures_processed: usize = 0;
    let last_signature: String;
    let mut non_parsed_transactions_set: HashSet<String> = HashSet::new();
    let mut non_parsed_transactions: Vec<String> = vec![];
    let mut user_transactions: HashMap<String, rpc_wrapper::StakingUserDataAccount> =
        match signatures {
            Some(signatures) => {
                total_signatures_processed = signatures.len();
                last_signature = signatures.last().unwrap().to_string();
                rpc_wrapper::get_user_data_account_with_transactions(
                    rpc_client.inner(),
                    &staking_config.staking_account_address,
                    &staking_config.fee_payer_address,
                    signatures,
                    &mut non_parsed_transactions_set,
                    &mut non_parsed_transactions,
                    true,
                )
                .await
            }
            None => {
                last_signature = String::from("");
                HashMap::new()
            }
        };
    rpc_wrapper::get_staking_user_account_info(rpc_client.inner(), &mut user_transactions).await;
    Json((
        user_transactions,
        total_signatures_processed,
        last_signature,
        non_parsed_transactions,
    ))
}

#[post("/get_transactions_info?<to_retry>", data = "<transaction_signatures>")]
async fn get_transactions_info(
    rpc_client: &State<RpcClient>,
    staking_config: &State<staking::StakingConfig>,
    to_retry: bool,
    transaction_signatures: Json<Vec<String>>,
) -> Json<(
    HashMap<String, rpc_wrapper::StakingUserDataAccount>,
    Vec<String>,
)> {
    let transaction_signatures = transaction_signatures.0;
    let mut non_parsed_transactions_set: HashSet<String> = HashSet::new();
    let mut non_parsed_transactions: Vec<String> = vec![];
    let mut user_transactions: HashMap<String, rpc_wrapper::StakingUserDataAccount> =
        rpc_wrapper::get_user_data_account_with_transactions(
            rpc_client.inner(),
            &staking_config.staking_account_address,
            &staking_config.fee_payer_address,
            transaction_signatures,
            &mut non_parsed_transactions_set,
            &mut non_parsed_transactions,
            to_retry,
        )
        .await;

    rpc_wrapper::get_staking_user_account_info(rpc_client.inner(), &mut user_transactions).await;
    Json((user_transactions, non_parsed_transactions))
}

#[post["/get_staking_user_account_info", data = "<users_data>"]]
async fn get_staking_user_account_info(
    rpc_client: &State<RpcClient>,
    users_data: Json<Vec<(String, String, String)>>,
) -> Json<HashMap<String, rpc_wrapper::StakingUserDataAccount>> {
    let mut transactions: HashMap<String, rpc_wrapper::StakingUserDataAccount> = HashMap::new();
    for user_data in users_data.0 {
        transactions.insert(
            user_data.0.to_owned(),
            rpc_wrapper::StakingUserDataAccount::new(user_data.1, user_data.2, vec![], true),
        );
    }
    rpc_wrapper::get_staking_user_account_info(rpc_client.inner(), &mut transactions).await;
    Json(transactions)
}

#[get("/get_staking_data_account_info?<staking_account>")]
async fn get_staking_data_account_info(
    rpc_client: &State<RpcClient>,
    staking_account: &str,
) -> Json<Result<rpc_wrapper::StakingDataAccount, String>> {
    Json(rpc_wrapper::get_staking_data_account_info(
        rpc_client.inner(),
        staking_account,
    ))
}

#[get("/accrue_interest")]
async fn accrue_interest_txn(
    rpc_client: &State<RpcClient>,
    staking_config: &State<staking::StakingConfig>,
) -> Json<Result<String, String>> {
    let program_id = staking_config
        .staking_program_address
        .parse::<Pubkey>()
        .unwrap();

    let accrue_interest_instruction = staking::instruction::ACCRUE_INTEREST;

    let instruction_data = instruction::InstructionAccrueInterest {
        instruction_type: accrue_interest_instruction,
    };

    let blockhash = rpc_client.get_latest_blockhash().unwrap();

    let accrue_interest_instruction = instruction::get_accrued_interest(
        program_id,
        &instruction_data,
        &staking_config.staking_account_address,
        &staking_config.staking_holding_wallet,
    );

    let fee_payer = staking_config.fee_payer_address.parse::<Pubkey>().unwrap();
    //let keypair = Keypair::from_base58_string(&staking_config.fee_payer_private_key);
    let message =
        Message::new_with_blockhash(&[accrue_interest_instruction], Some(&fee_payer), &blockhash);
    let trx: Vec<u8> = bincode::serialize(&Transaction::new_unsigned(message)).unwrap();
    let base64_txn: String = general_purpose::STANDARD.encode(&trx);

    routes::transaction::send(rpc_client, staking_config, base64_txn).await
}

#[get("/fund_staking?<amount>")]
async fn fund_staking(
    rpc_client: &State<RpcClient>,
    staking_config: &State<staking::StakingConfig>,
    amount: u128,
) -> Json<Result<String, String>> {
    let program_id = staking_config
        .staking_program_address
        .parse::<Pubkey>()
        .unwrap();

    //Fee payer publickey
    let fee_payer = staking_config.fee_payer_address.parse::<Pubkey>().unwrap();
    //Temp funding wallet private key
    let temp_funding_wallet_private_key = &staking_config
        .funding_wallet_private_key
        .parse::<String>()
        .unwrap();
    //Temp funding wallet public key
    let temp_funding_pubkey = staking_config
        .funding_wallet_address
        .parse::<String>()
        .unwrap();
    let staking_account_address = &staking_config.staking_account_address;
    let user_spl_token_owner = &temp_funding_pubkey;
    let staking_account_token_mint = &staking_config.staking_account_token_mint;
    let user_spl_token_account = rpc_wrapper::get_associated_account(
        &rpc_client,
        &user_spl_token_owner,
        &user_spl_token_owner,
        &staking_account_token_mint,
    )
    .0;

    let staking_holding_wallet = &staking_config.staking_holding_wallet;

    let fund_staking_instruction = staking::instruction::FUND_STAKING_WALLET;

    let instruction_data = instruction::InstructionFundStaking {
        instruction_type: fund_staking_instruction,
        amount,
    };

    let fund_staking_instruction = instruction::get_fund_staking(
        program_id,
        &instruction_data,
        staking_account_address,
        user_spl_token_account,
        user_spl_token_owner,
        staking_holding_wallet,
        staking_account_token_mint,
    );

    let blockhash = rpc_client.get_latest_blockhash().unwrap();

    let message =
        Message::new_with_blockhash(&[fund_staking_instruction], Some(&fee_payer), &blockhash);
    let txn = &mut Transaction::new_unsigned(message);

    //Sign by temp funding wallet pubkey
    let trx: Vec<u8> = bincode::serialize(&txn).unwrap();
    let base64_txn: String = general_purpose::STANDARD.encode(&trx);
    let updated_txn =
        routes::transaction::sign(temp_funding_wallet_private_key.to_owned(), base64_txn).await;

    //Sign and send by fee payer
    routes::transaction::send(rpc_client, staking_config, updated_txn).await
}

#[get("/get_notification_data")]
async fn get_notification_data(
    rpc_client: &State<RpcClient>,
    staking_config: &State<staking::StakingConfig>,
) -> Json<Result<String, String>> {
    let staking_account_address = staking_config
        .staking_account_address
        .parse::<String>()
        .unwrap();

    let fee_payer = staking_config.fee_payer_address.parse::<Pubkey>().unwrap();
    let temp_funding_pubkey = staking_config
        .funding_wallet_address
        .parse::<String>()
        .unwrap();

    let staking_holding_wallet = staking_config
        .staking_holding_wallet
        .parse::<Pubkey>()
        .unwrap();

    let account_sol_balance = match helper::get_sol_balance(rpc_client, &fee_payer) {
        Ok(balance) => balance,
        Err(err) => {
            warn!("Error getting account sol balance: {:?}", err);
            return Json(Err(err));
        }
    };

    let holding_wallet_gari_balance =
        match helper::get_mint_balance(rpc_client, &staking_holding_wallet) {
            Ok(balance) => balance,
            Err(err) => {
                warn!("Error getting holding wallet gari_balance: {:?}", err);
                return Json(Err(err));
            }
        };
    let staking_data =
        match helper::get_staking_data_account_info(rpc_client, &staking_account_address) {
            Ok(data) => data,
            Err(err) => {
                warn!("Error getting staking_data: {:?}", err);
                return Json(Err(err));
            }
        };
    let base: u64 = 10;
    let decimals: u64 = base.pow(9 as u32);

    let total_staked = staking_data.total_staked;
    let sol_balance_str = std::format!(
        "{} {:.3}",
        "Fee Payer Sol Balance:",
        account_sol_balance as f64 / decimals as f64
    );
    let holding_wallet_gari_balance_str = std::format!(
        "{} {:.3}",
        "Holding Wallet Gari Balance:",
        holding_wallet_gari_balance
    );
    let total_staked_balance_str = std::format!(
        "{} {:.3}",
        "Total Staked: ",
        total_staked as f64 / decimals as f64
    );

    let next_timestamp = staking_data.last_interest_accrued_timestamp
        + staking_config.interest_buffer.parse::<i64>().unwrap() * clock::SECONDS_PER_DAY as i64;

    let (unminted_interest, _) = match dto::calculate_accrued_interest(
        staking_data.last_interest_accrued_timestamp,
        next_timestamp,
        staking_data.total_staked,
        staking_data.interest_rate_hourly,
    ) {
        Ok(unminted_interest) => unminted_interest,
        Err(err) => {
            warn!("Error calculating interest");
            return Json(Err(err));
        }
    };

    let mut user_action = dto::UserActions::None;

    //Get fee payer gari balance
    let temp_funding_wallet_gari_balance = match helper::get_wallet_mint_balance(
        &rpc_client,
        &temp_funding_pubkey,
        &staking_config.staking_account_token_mint,
    ) {
        Ok(data) => data,
        Err(err) => return Json(Err(err)),
    };

    let temp_funding_wallet_gari_balance_str = std::format!(
        "{} {:.3}",
        "Temp Funding wallet Wallet Gari Balance:",
        temp_funding_wallet_gari_balance
    );
    let temp_funding_wallet_gari_balance_u128 =
        (temp_funding_wallet_gari_balance * decimals as f64) as u128;
    let funding_wallet_buffer =
        staking_config.funding_wallet_buffer.parse::<u64>().unwrap() * decimals;
    //required funding wallet balance = current fund wallet balance - (staking balance+ unminted interest + buffer) > 0

    let diff: u128 = helper::get_funding_amount_required(
        total_staked,
        unminted_interest,
        funding_wallet_buffer,
        holding_wallet_gari_balance,
    ) as u128;
    //if not check if  fee payer gari balance > required funding wallet balance

    let transfer_value = if diff < temp_funding_wallet_gari_balance_u128 {
        diff
    } else {
        temp_funding_wallet_gari_balance_u128
    };
    info!(
        "diff={:.3}, temp_funding_wallet_gari_balance_u64={:?}",
        (diff as f64 / decimals as f64) as f64,
        temp_funding_wallet_gari_balance_u128
    );
    if transfer_value > 0 {
        match fund_staking(
            &rpc_client,
            &staking_config,
            //&staking_config.fee_payer_address,
            transfer_value,
            //100,
        )
        .await
        .0
        {
            Ok(result) => info!("Funded staking wallet: {:?}", result),
            Err(error) => warn!("Error {}", error),
        };
    }
    if diff > temp_funding_wallet_gari_balance_u128 {
        //Transfer required amount if possible
        //
        user_action = dto::UserActions::FundFeePayer;
    }
    //If not user action = Fund Gari Fee Payer with money

    let needed_str = std::format!(
        "{} {} {} {:.3}",
        "Interest needed for next",
        staking_config.interest_buffer.parse::<i64>().unwrap(),
        "days in holding wallet: ",
        unminted_interest as f64 / decimals as f64
    );

    let last_interest_accrued_time =
        match NaiveDateTime::from_timestamp_opt(staking_data.last_interest_accrued_timestamp, 0) {
            Some(val) => std::format!(
                "{}",
                DateTime::<Utc>::from_utc(val, Utc).format("%Y-%m-%d %H:%M:%S")
            ),
            None => String::from("N/A"),
        };

    let newdate = std::format!(
        "{} {} {}",
        "Last accrued interest date: ",
        last_interest_accrued_time,
        "UTC"
    );

    let serialized_data = match rocket::serde::json::to_string(&dto::NotificationData {
        sol_balance: sol_balance_str,
        gari_balance: temp_funding_wallet_gari_balance_str,
        holding_wallet_balance: holding_wallet_gari_balance_str,
        total_staked_balance: total_staked_balance_str,
        needed_interest: needed_str,
        last_interest_accrued_time: newdate,
        user_action: std::format!("Required Action: {}", user_action.to_string()),
    }) {
        Ok(json) => json,
        Err(err) => return Json(Err(format!("Error in deserializing data {}", err))),
    };
    Json(Ok(serialized_data))
}

#[get("/")]
async fn health_ping() -> &'static str {
    ""
}

#[launch]
fn rocket() -> _ {
    let staking_config = Config::figment()
        .extract::<staking::StakingConfig>()
        .unwrap();
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", &staking_config.solana_web_api_rust_log);
    }

    pretty_env_logger::init_timed();
    let rpc_client = RpcClient::new(&staking_config.on_chain_endpoint);
    rocket::build()
        .manage(rpc_client)
        .manage(staking_config)
        .attach(routes::mount())
        .mount(
            "/",
            routes![
                get_transactions_and_account_info,
                get_transactions_info,
                get_staking_user_account_info,
                get_staking_data_account_info,
                health_ping,
                accrue_interest_txn,
                get_notification_data,
                fund_staking
            ],
        )
}

#[cfg(test)]
mod rpc_wrapper_tests;
