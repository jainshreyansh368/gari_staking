use crate::{instruction, rpc_wrapper, staking};
use base64::{engine::general_purpose, Engine as _};
use log::warn;
use rocket::{serde::json::Json, State};
use solana_client::{
    rpc_client::RpcClient, rpc_config::RpcSendTransactionConfig,
    rpc_config::RpcSimulateTransactionConfig,
};
use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
    message::Message,
    pubkey::Pubkey,
    signature::Keypair,
    transaction::{Transaction, TransactionError},
};

#[get(
    "/get_encoded_transaction?<user_spl_token_owner>&<amount>&<instruction_type>&<staking_holding_wallet>&<staking_user_data_account>&<user_spl_token_account>"
)]
pub async fn encode(
    rpc_client: &State<RpcClient>,
    staking_config: &State<staking::StakingConfig>,
    user_spl_token_owner: String,
    amount: u64,
    instruction_type: String,
    staking_holding_wallet: String,
    staking_user_data_account: Option<String>,
    user_spl_token_account: Option<String>,
) -> Json<Result<String, String>> {
    let program_id = staking_config
        .staking_program_address
        .parse::<Pubkey>()
        .unwrap();
    let staking_instruction = if instruction_type.eq("stake") {
        staking::instruction::STAKE
    } else if instruction_type.eq("unstake") {
        staking::instruction::UNSTAKE
    } else {
        return Json(Err("Wrong instruction type".to_string()));
    };
    let instruction_data = instruction::InstructionDataWithAmount {
        instruction_type: staking_instruction,
        amount: amount,
    };

    // if not in db, then initialize
    let staking_user_data_account = if staking_user_data_account.is_none() {
        // expensive calculation
        let user_pubkey = match user_spl_token_owner.parse::<Pubkey>() {
            Ok(pubkey) => pubkey,
            Err(error) => {
                warn!("Invalid user pubkey {}: {}", user_spl_token_owner, error);
                return Json(Err("Invalid Public key sent".to_string()));
            }
        };
        let (account, _bump_seed) = Pubkey::find_program_address(
            &[
                &staking_config
                    .staking_account_address
                    .parse::<Pubkey>()
                    .unwrap()
                    .to_bytes(),
                &user_pubkey.to_bytes(),
            ],
            &staking_config
                .staking_program_address
                .parse::<Pubkey>()
                .unwrap(),
        );
        account
    } else {
        staking_user_data_account
            .unwrap()
            .parse::<Pubkey>()
            .unwrap()
    };

    let (user_spl_token_account, associated_instruction) = if user_spl_token_account.is_none() {
        rpc_wrapper::get_associated_account(
            rpc_client,
            &user_spl_token_owner,
            &staking_config.fee_payer_address,
            &staking_config.staking_account_token_mint,
        )
    } else {
        (
            user_spl_token_account.unwrap().parse::<Pubkey>().unwrap(),
            None,
        )
    };
    let call_init =
        !rpc_wrapper::is_staking_user_account_initialized(rpc_client, &staking_user_data_account);

    let main_instruction = match staking_instruction {
        staking::instruction::STAKE => instruction::get_stake(
            program_id,
            &instruction_data,
            staking_user_data_account,
            user_spl_token_account,
            &user_spl_token_owner,
            &staking_config.staking_account_address,
            &staking_holding_wallet,
            &staking_config.staking_account_token_mint,
        ),
        _ => instruction::get_unstake(
            program_id,
            &instruction_data,
            staking_user_data_account,
            user_spl_token_account,
            &user_spl_token_owner,
            &staking_config.staking_account_address,
            &staking_holding_wallet,
            &staking_config.staking_holding_wallet_owner,
            &staking_config.staking_account_token_mint,
        ),
    };

    let blockhash = rpc_client.get_latest_blockhash().unwrap();
    let init_instruction = instruction::get_init(
        program_id,
        &instruction::InstructionDataInit {
            instruction_type: staking::instruction::INIT_STAKING_USER,
        },
        staking_user_data_account,
        user_spl_token_account,
        &user_spl_token_owner,
        &staking_config.fee_payer_address,
        &staking_config.staking_account_address,
        &staking_config.staking_account_token_mint,
    );
    let instructions = match associated_instruction {
        Some(associated_instruction) => {
            vec![associated_instruction, init_instruction, main_instruction]
        }
        None => {
            if call_init {
                vec![init_instruction, main_instruction]
            } else {
                vec![main_instruction]
            }
        }
    };

    let fee_payer = staking_config.fee_payer_address.parse::<Pubkey>().unwrap();
    let message = Message::new_with_blockhash(&instructions, Some(&fee_payer), &blockhash);
    let trx: Vec<u8> = bincode::serialize(&Transaction::new_unsigned(message)).unwrap();

    Json(Ok(general_purpose::STANDARD.encode(trx)))
}

#[post("/send_transaction", data = "<encoded_transaction>")]
pub async fn send(
    rpc_client: &State<RpcClient>,
    staking_config: &State<staking::StakingConfig>,
    encoded_transaction: String,
) -> Json<Result<String, String>> {
    let mut transaction = bincode::deserialize::<Transaction>(
        general_purpose::STANDARD
            .decode(encoded_transaction)
            .unwrap()
            .as_slice(),
    )
    .unwrap();

    let keypair = Keypair::from_base58_string(&staking_config.fee_payer_private_key);
    transaction.partial_sign(&[&keypair], transaction.message.recent_blockhash);

    let simulation_result = if staking_config.send_transaction_simulate {
        let rpc_simulate_transaction_config = RpcSimulateTransactionConfig {
            sig_verify: true,
            commitment: Some(CommitmentConfig::confirmed()),
            ..RpcSimulateTransactionConfig::default()
        };

        match rpc_client
            .simulate_transaction_with_config(&transaction, rpc_simulate_transaction_config)
        {
            Ok(result) => match result.value.err {
                Some(TransactionError::InstructionError(_, _)) => {
                    let mut log = String::new();
                    for l in result.value.logs.unwrap() {
                        log.push_str(&l);
                        log.push_str("  ");
                    }

                    Some(Json(Err(format!("{}", log))))
                }
                _ => None,
            },
            Err(error) => Some(Json(Err(error.to_string()))),
        }
    } else {
        None
    };

    if simulation_result.is_some() {
        return simulation_result.unwrap();
    }

    let rpc_send_transaction_config = RpcSendTransactionConfig {
        skip_preflight: false,
        preflight_commitment: Some(CommitmentLevel::Confirmed),
        ..RpcSendTransactionConfig::default()
    };

    match rpc_client.send_transaction_with_config(&transaction, rpc_send_transaction_config) {
        Ok(result) => {
            let result = result.to_string();
            if result.eq(instruction::SOL_ADDRESS) {
                Json(Err("Transaction dropped!".to_owned()))
            } else {
                Json(Ok(result))
            }
        }
        Err(error) => Json(Err(error.to_string())),
    }
}

#[post("/sign_transaction/<user_private_key>", data = "<encoded_transaction>")]
pub async fn sign(user_private_key: String, encoded_transaction: String) -> String {
    let mut transaction = bincode::deserialize::<Transaction>(
        general_purpose::STANDARD
            .decode(encoded_transaction)
            .unwrap()
            .as_slice(),
    )
    .unwrap();

    let keypair = Keypair::from_base58_string(&user_private_key);
    transaction.partial_sign(&[&keypair], transaction.message.recent_blockhash);
    let trx: Vec<u8> = bincode::serialize(&transaction).unwrap();
    general_purpose::STANDARD.encode(trx)
}

#[cfg(test)]
mod tests {
    // dec macro for constructing the rocket state with mock RpcClient inner
    macro_rules! set_rocket_state_with_mock_rpc {
        ($name:ident, $mock_type:literal) => {
            // construct a mock RpcClient which will return successfull generic responses
            let rpc_client = crate::RpcClient::new_mock($mock_type.to_string());
            // create rocket instance with rpc_client state
            let rocket = rocket::build().manage(rpc_client);
            // create a state from rocket instance
            // because state cannot be constructed directly as State's inner is not public
            // *Workaround*
            let $name = crate::State::from(rocket.state::<crate::RpcClient>().unwrap());
        };
    }

    // dec macro for constructing the rocket state with mock StakingConfig inner
    macro_rules! set_rocket_state_with_staking_config {
        ($name:ident) => {
            // staking config for testing
            let staking_config = crate::staking::StakingConfig {
                on_chain_endpoint: String::from("https://api.mainnet-beta.solana.com"),
                staking_program_address: String::from("4F531NBnaBYquFVpde2Atc3NpcTcRYkNRgcoZ2XT1Tza"),
                staking_account_address: String::from("G2fV9BvL36qxVUSGmCmnc6ab8TPg4S1cGajXewbWM6Rw"),
                staking_account_token_mint: String::from("7gjQaUHVdP8m7BvrFWyPkM7L3H9p4umwm3F56q1qyLk1"),
                staking_holding_wallet_owner: String::from("C8EfwCYCxWkaT2yZWECip93dcxbMiHwgYLcazgkfXuhW"),
                solana_web_api_rust_log: String::from("info"),
                fee_payer_address: String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw"),
                fee_payer_private_key: String::from("nNL6jFkycDn4EpFiqqbYzU6A5DWaH9wEr8oJZcjimXM8q3BEu6CYnEcuvm6GdfNN9kkw9eG1JKrYqap6ZFtEPob"),
                send_transaction_simulate: true,
                staking_holding_wallet: String::from("Hmn35143VCdGu2dGDSwefHTLtjFxJMtUr9BrpB3PACV5"),
                funding_wallet_buffer: String::from("10"),
                interest_buffer: String::from("7"),
                funding_wallet_private_key: String::from("nNL6jFkycDn4EpFiqqbYzU6A5DWaH9wEr8oJZcjimXM8q3BEu6CYnEcuvm6GdfNN9kkw9eG1JKrYqap6ZFtEPob"),
                funding_wallet_address: String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw")
            };
            // create rocket instance with rpc_client state
            let rocket = rocket::build().manage(staking_config);
            // create a state from rocket instance
            // because state cannot be constructed directly as State's inner is not public
            // *Workaround*
            let $name = crate::State::from(rocket.state::<crate::staking::StakingConfig>().unwrap());
        };
    }

    mod encode_fn {
        use crate::routes::transaction::encode;

        // passing tests which should not panic on unwrap() and returns Ok()
        mod passing {
            #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
            async fn test_encode_ins_main_init_ass_tkn() {
                set_rocket_state_with_mock_rpc!(state, "succeds");
                set_rocket_state_with_staking_config!(staking_config);

                let serialized_ins_from_encode = super::encode(
                    state,
                    staking_config,
                    String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw"),
                    10,
                    String::from("stake"),
                    String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw"),
                    None,
                    None,
                )
                .await
                .into_inner();

                // known serialized instruction for the above passed input params into the encode() function
                let known_serialized_ins = String::from("AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAUKnfaQgu8/NqZg8EzPkRyUZ+S6ympgVfwZQF+/qJwB1h5jU7joYwTl/ceUs9HTLVSkAyFfWhiE4Xb07mWw6fpnqpEeScqIj1zdHH7tpqtrl6QNsY/t+ecO8FZ7CnsRBz+Lo46iSgUm6q7LRqr1DbuHku5u5eCoBzzVwiIaUI9ohInfTGZptWA96Rdmk+rP2bqkeU9rSjsO3RymCvwi8hGQigAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABqfVFxksXFEhjMlMPUrxf1ja7gibof1E49vZigAAAAAG3fbh12Whk9nL4UbO63msHLSF7V9bN5E6jPWFfv8AqTAtwQhP40b6rABWJYg5H2RKF7QTMPdEMTTNLZrcYP1TjJclj04kifG7PRApFI4NgwtaE5na/xCEBI572Nvp+FlfgJDCob41MPEOMFuZgP2vgHGGCPVK6F2FaWXPk2y+PwMJBgADAAEFBwEACAgCAwAABAEFBgjq533Ejjia6ggHAgMABAABBxDOsMoSyNGzbAoAAAAAAAAA");

                assert!(serialized_ins_from_encode.is_ok());

                assert_eq!(serialized_ins_from_encode.unwrap(), known_serialized_ins);
            }

            #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
            async fn test_encode_ins_main_init() {
                set_rocket_state_with_mock_rpc!(state, "succeds");
                set_rocket_state_with_staking_config!(staking_config);

                let serialized_ins_from_encode = super::encode(
                    state,
                    staking_config,
                    String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw"),
                    10,
                    String::from("stake"),
                    String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw"),
                    None,
                    Some(String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw")),
                )
                .await
                .into_inner();

                // known serialized instruction for the above passed input params into the encode() function
                let known_serialized_ins = String::from("AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAQInfaQgu8/NqZg8EzPkRyUZ+S6ympgVfwZQF+/qJwB1h5jU7joYwTl/ceUs9HTLVSkAyFfWhiE4Xb07mWw6fpnqpEeScqIj1zdHH7tpqtrl6QNsY/t+ecO8FZ7CnsRBz+L30xmabVgPekXZpPqz9m6pHlPa0o7Dt0cpgr8IvIRkIoAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAan1RcZLFxRIYzJTD1K8X9Y2u4Im6H9ROPb2YoAAAAABt324ddloZPZy+FGzut5rBy0he1fWzeROoz1hX7/AKkwLcEIT+NG+qwAViWIOR9kShe0EzD3RDE0zS2a3GD9U1+AkMKhvjUw8Q4wW5mA/a+AcYYI9UroXYVpZc+TbL4/AgcIAgAAAAMBBAUI6ud9xI44muoHBwIAAAMAAQYQzrDKEsjRs2wKAAAAAAAAAA==");

                assert!(serialized_ins_from_encode.is_ok());

                assert_eq!(serialized_ins_from_encode.unwrap(), known_serialized_ins);
            }

            #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
            async fn test_encode_ins_main() {
                set_rocket_state_with_mock_rpc!(state, "succeds");
                set_rocket_state_with_staking_config!(staking_config);

                let serialized_ins_from_encode = super::encode(
                    state,
                    staking_config,
                    String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw"),
                    10,
                    String::from("stake"),
                    String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw"),
                    Some(String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw")),
                    Some(String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw")),
                )
                .await
                .into_inner();

                // known serialized instruction for the above passed input params into the encode() function
                let known_serialized_ins = String::from("AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAQHnfaQgu8/NqZg8EzPkRyUZ+S6ympgVfwZQF+/qJwB1h5jU7joYwTl/ceUs9HTLVSkAyFfWhiE4Xb07mWw6fpnqt9MZmm1YD3pF2aT6s/ZuqR5T2tKOw7dHKYK/CLyEZCKAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGp9UXGSxcUSGMyUw9SvF/WNruCJuh/UTj29mKAAAAAAbd9uHXZaGT2cvhRs7reawctIXtX1s3kTqM9YV+/wCpMC3BCE/jRvqsAFYliDkfZEoXtBMw90QxNM0tmtxg/VNfgJDCob41MPEOMFuZgP2vgHGGCPVK6F2FaWXPk2y+PwIGCAAAAAACAQMECOrnfcSOOJrqBgcAAAACAAEFEM6wyhLI0bNsCgAAAAAAAAA=");

                assert!(serialized_ins_from_encode.is_ok());

                assert_eq!(serialized_ins_from_encode.unwrap(), known_serialized_ins);
            }
        }

        // failing tests which should panic on unwraps or returns Err()
        mod failing {
            #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
            async fn test_encode_invalid_instruction_input_param() {
                set_rocket_state_with_mock_rpc!(state, "succeds");
                set_rocket_state_with_staking_config!(staking_config);

                let serialized_ins_from_encode = super::encode(
                    state,
                    staking_config,
                    String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw"),
                    10,
                    String::from("invalidinstype"),
                    String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw"),
                    Some(String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw")),
                    Some(String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw")),
                )
                .await
                .into_inner();

                assert!(serialized_ins_from_encode.is_err());

                assert_eq!(
                    serialized_ins_from_encode.unwrap_err(),
                    String::from("Wrong instruction type")
                );
            }

            #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
            async fn test_encode_invalid_user_pubkey_input_param() {
                set_rocket_state_with_mock_rpc!(state, "succeds");
                set_rocket_state_with_staking_config!(staking_config);

                let serialized_ins_from_encode = super::encode(
                    state,
                    staking_config,
                    String::from("InvalidSplTokenOwnerPubkey111111111111111111"),
                    10,
                    String::from("stake"),
                    String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw"),
                    None,
                    Some(String::from("Bdd59KsthFZxqMjBbpK9Qd9FUpkmLMxTcpNystMv1CXw")),
                )
                .await
                .into_inner();

                assert!(serialized_ins_from_encode.is_err());

                assert_eq!(
                    serialized_ins_from_encode.unwrap_err(),
                    String::from("Invalid Public key sent")
                );
            }
        }
    }

    mod send_fn {
        use crate::routes::transaction::send;

        // passing tests for send function
        mod passing {
            #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
            async fn test_send_valid_serialized_ins() {
                set_rocket_state_with_mock_rpc!(state, "succeds");
                set_rocket_state_with_staking_config!(staking_config);
                let known_serialized_ins = String::from("AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAQHnfaQgu8/NqZg8EzPkRyUZ+S6ympgVfwZQF+/qJwB1h5jU7joYwTl/ceUs9HTLVSkAyFfWhiE4Xb07mWw6fpnqt9MZmm1YD3pF2aT6s/ZuqR5T2tKOw7dHKYK/CLyEZCKAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGp9UXGSxcUSGMyUw9SvF/WNruCJuh/UTj29mKAAAAAAbd9uHXZaGT2cvhRs7reawctIXtX1s3kTqM9YV+/wCpMC3BCE/jRvqsAFYliDkfZEoXtBMw90QxNM0tmtxg/VNfgJDCob41MPEOMFuZgP2vgHGGCPVK6F2FaWXPk2y+PwIGCAAAAAACAQMECOrnfcSOOJrqBgcAAAACAAEFEM6wyhLI0bNsCgAAAAAAAAA=");

                let result_send = super::send(state, staking_config, known_serialized_ins)
                    .await
                    .into_inner();

                assert!(result_send.is_ok());
                assert_eq!(result_send.unwrap(), String::from("4q4V6FbgBxoNi6G7yn1c4UcfsmdtAkCM7iNCwHxxrELnfosNsyAvsgwRHRGzG6uzjzv3EDb19HBQK3UsQFJpK8MV"));
            }
        }

        // failing tests for send function, should fail on unwraps or Err returns
        mod failing {
            #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
            #[should_panic]
            async fn test_send_invalid_serialized_ins() {
                set_rocket_state_with_mock_rpc!(state, "succeds");
                set_rocket_state_with_staking_config!(staking_config);
                let known_serialized_ins = String::from("bbbbbbAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAQHnfaQgu8/NqZg8EzPkRyUZ+S6ympgVfwZQF+/qJwB1h5jU7joYwTl/ceUs9HTLVSkAyFfWhiE4Xb07mWw6fpnqt9MZmm1YD3pF2aT6s/ZuqR5T2tKOw7dHKYK/CLyEZCKAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGp9UXGSxcUSGMyUw9SvF/WNruCJuh/UTj29mKAAAAAAbd9uHXZaGT2cvhRs7reawctIXtX1s3kTqM9YV+/wCpMC3BCE/jRvqsAFYliDkfZEoXtBMw90QxNM0tmtxg/VNfgJDCob41MPEOMFuZgP2vgHGGCPVK6F2FaWXPk2y+PwIGCAAAAAACAQMECOrnfcSOOJrqBgcAAAACAAEFEM6wyhLI0bNsCgAAAAAAAAA=");

                super::send(state, staking_config, known_serialized_ins).await;
            }

            #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
            async fn test_send_failing_rpc_simulation() {
                set_rocket_state_with_mock_rpc!(state, "fails");
                set_rocket_state_with_staking_config!(staking_config);
                let known_serialized_ins = String::from("AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAQHnfaQgu8/NqZg8EzPkRyUZ+S6ympgVfwZQF+/qJwB1h5jU7joYwTl/ceUs9HTLVSkAyFfWhiE4Xb07mWw6fpnqt9MZmm1YD3pF2aT6s/ZuqR5T2tKOw7dHKYK/CLyEZCKAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGp9UXGSxcUSGMyUw9SvF/WNruCJuh/UTj29mKAAAAAAbd9uHXZaGT2cvhRs7reawctIXtX1s3kTqM9YV+/wCpMC3BCE/jRvqsAFYliDkfZEoXtBMw90QxNM0tmtxg/VNfgJDCob41MPEOMFuZgP2vgHGGCPVK6F2FaWXPk2y+PwIGCAAAAAACAQMECOrnfcSOOJrqBgcAAAACAAEFEM6wyhLI0bNsCgAAAAAAAAA=");

                let result_send = super::send(state, staking_config, known_serialized_ins)
                    .await
                    .into_inner();

                assert!(result_send.is_err());
                assert_eq!(result_send.unwrap_err(), String::from("RPC request error: cluster version query failed: invalid type: null, expected struct RpcVersionInfo"));
            }
        }
    }
    mod sign_fn {
        use crate::routes::transaction::sign;

        // passing tests for sign function
        mod passing {
            #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
            async fn test_sign_valid_instruction() {
                let user_private_key = String::from("nNL6jFkycDn4EpFiqqbYzU6A5DWaH9wEr8oJZcjimXM8q3BEu6CYnEcuvm6GdfNN9kkw9eG1JKrYqap6ZFtEPob");

                // known serialized ins
                let encoded_serialized_ins = String::from("AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAUKnfaQgu8/NqZg8EzPkRyUZ+S6ympgVfwZQF+/qJwB1h5jU7joYwTl/ceUs9HTLVSkAyFfWhiE4Xb07mWw6fpnqpEeScqIj1zdHH7tpqtrl6QNsY/t+ecO8FZ7CnsRBz+Lo46iSgUm6q7LRqr1DbuHku5u5eCoBzzVwiIaUI9ohInfTGZptWA96Rdmk+rP2bqkeU9rSjsO3RymCvwi8hGQigAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABqfVFxksXFEhjMlMPUrxf1ja7gibof1E49vZigAAAAAG3fbh12Whk9nL4UbO63msHLSF7V9bN5E6jPWFfv8AqTAtwQhP40b6rABWJYg5H2RKF7QTMPdEMTTNLZrcYP1TjJclj04kifG7PRApFI4NgwtaE5na/xCEBI572Nvp+FlfgJDCob41MPEOMFuZgP2vgHGGCPVK6F2FaWXPk2y+PwMJBgADAAEFBwEACAgCAwAABAEFBgjq533Ejjia6ggHAgMABAABBxDOsMoSyNGzbAoAAAAAAAAA");

                let signed_encoded_transaction = String::from("AVPpUwul2JbS8CZMvln/S3IUeTsJPx/RD4mbTspBLHtTxSkauAkU8ROjwaAUYDsxLzBEztiUfyrjUTD7OvoYlQwBAAUKnfaQgu8/NqZg8EzPkRyUZ+S6ympgVfwZQF+/qJwB1h5jU7joYwTl/ceUs9HTLVSkAyFfWhiE4Xb07mWw6fpnqpEeScqIj1zdHH7tpqtrl6QNsY/t+ecO8FZ7CnsRBz+Lo46iSgUm6q7LRqr1DbuHku5u5eCoBzzVwiIaUI9ohInfTGZptWA96Rdmk+rP2bqkeU9rSjsO3RymCvwi8hGQigAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABqfVFxksXFEhjMlMPUrxf1ja7gibof1E49vZigAAAAAG3fbh12Whk9nL4UbO63msHLSF7V9bN5E6jPWFfv8AqTAtwQhP40b6rABWJYg5H2RKF7QTMPdEMTTNLZrcYP1TjJclj04kifG7PRApFI4NgwtaE5na/xCEBI572Nvp+FlfgJDCob41MPEOMFuZgP2vgHGGCPVK6F2FaWXPk2y+PwMJBgADAAEFBwEACAgCAwAABAEFBgjq533Ejjia6ggHAgMABAABBxDOsMoSyNGzbAoAAAAAAAAA");

                assert_eq!(
                    super::sign(user_private_key, encoded_serialized_ins).await,
                    signed_encoded_transaction
                );
            }
        }

        // failing tests for sign function, should fail on unwraps or Err returns
        mod failing {
            #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
            #[should_panic]
            async fn test_sign_invalid_instruction() {
                let user_private_key = String::from("nNL6jFkycDn4EpFiqqbYzU6A5DWaH9wEr8oJZcjimXM8q3BEu6CYnEcuvm6GdfNN9kkw9eG1JKrYqap6ZFtEPob");

                // invalid serialized ins
                let encoded_serialized_ins = String::from("BBBBQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAUKnfaQgu8/NqZg8EzPkRyUZ+S6ympgVfwZQF+/qJwB1h5jU7joYwTl/ceUs9HTLVSkAyFfWhiE4Xb07mWw6fpnqpEeScqIj1zdHH7tpqtrl6QNsY/t+ecO8FZ7CnsRBz+Lo46iSgUm6q7LRqr1DbuHku5u5eCoBzzVwiIaUI9ohInfTGZptWA96Rdmk+rP2bqkeU9rSjsO3RymCvwi8hGQigAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABqfVFxksXFEhjMlMPUrxf1ja7gibof1E49vZigAAAAAG3fbh12Whk9nL4UbO63msHLSF7V9bN5E6jPWFfv8AqTAtwQhP40b6rABWJYg5H2RKF7QTMPdEMTTNLZrcYP1TjJclj04kifG7PRApFI4NgwtaE5na/xCEBI572Nvp+FlfgJDCob41MPEOMFuZgP2vgHGGCPVK6F2FaWXPk2y+PwMJBgADAAEFBwEACAgCAwAABAEFBgjq533Ejjia6ggHAgMABAABBxDOsMoSyNGzbAoAAAAAAAAA");

                super::sign(user_private_key, encoded_serialized_ins).await;
            }
        }
    }
}
