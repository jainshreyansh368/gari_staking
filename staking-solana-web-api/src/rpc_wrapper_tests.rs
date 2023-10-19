// Unit test to cover rpc_wrapper module

use super::*;
use crate::rpc_wrapper::*;
use std::collections::HashSet;

#[test]
fn calculate_est_apy_953_871() {
    assert_eq!(rpc_wrapper::StakingDataAccount::calculate_est_apy(953), 871);
}

#[test]
fn calculate_est_apy_1088_1000() {
    assert_eq!(
        rpc_wrapper::StakingDataAccount::calculate_est_apy(1088),
        1000
    );
}

#[test]
fn calculate_est_apy_600_540() {
    assert_eq!(rpc_wrapper::StakingDataAccount::calculate_est_apy(600), 540);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_get_transactions() {
    let staking_config = Config::figment()
        .extract::<staking::StakingConfig>()
        .unwrap();

    let rpc_client: RpcClient = RpcClient::new(&staking_config.on_chain_endpoint);
    let staking_account_address: &str = &staking_config.staking_account_address;
    let fee_payer_address: &str = &staking_config.fee_payer_address;
    let signatures: Vec<String> = vec![
        "2BwHVhHNpJVBurBy6VTfLrsmPTZ2ct7C4Q8v13wSveQU9BMJdhcNjCUfKXjTqpTVSnEht652Fn4rfaU8Wuu1RxoU"
            .to_owned(),
        "5kGoHLGttRcH77pw5YE7njAD5LtWtibjwBWbovX2osyaGRrigG3ssv3ZrpwJhH18g7B9UYXGScFiB6T9TTBLcZPW"
            .to_owned(),
        "pTtz6aBovWs7Y3PA7WqfH3pwBBVdG2RxyDxA6nFPmR413GjmHk4ttCNPGzYqax3hWfp1JpK7nSavi5ZTGNbLYpL"
            .to_owned(),
    ];

    std::env::set_var("RUST_LOG", "info");
    pretty_env_logger::init();
    let mut non_parsable_transactions_set: HashSet<String> = HashSet::new();
    let mut non_parsable_transactions: Vec<String> = vec![];

    let map = get_user_data_account_with_transactions(
        &rpc_client,
        staking_account_address,
        fee_payer_address,
        signatures,
        &mut non_parsable_transactions_set,
        &mut non_parsable_transactions,
        false,
    )
    .await;

    info!("{:?}", map);
    assert!(!map.is_empty());
}
