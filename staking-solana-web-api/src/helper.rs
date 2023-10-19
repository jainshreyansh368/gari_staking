use crate::{
    dto::{StakingData, StakingDataAccount},
    rpc_wrapper,
};
use borsh::BorshDeserialize;

use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;

pub fn get_sol_balance(rpc_client: &RpcClient, account_key: &Pubkey) -> Result<u64, String> {
    //SOLANA BALANCE

    match rpc_client.get_balance(&account_key) {
        Ok(balance) => Ok(balance),
        Err(err) => {
            let msg = std::format!("Error: {}", err.kind);
            warn!("{}", msg);
            Err(msg)
        }
    }
}

pub fn get_mint_balance(rpc_client: &RpcClient, account_key: &Pubkey) -> Result<f64, String> {
    //balance
    match rpc_client.get_token_account_balance(&account_key) {
        Ok(uibalance) => match uibalance.ui_amount {
            Some(x) => Ok(x),
            None => Ok(0 as f64),
        },
        Err(err) => {
            let msg = std::format!("Error: {}", err.kind);
            warn!("{}", msg);
            Err(msg)
        }
    }
}

pub fn get_staking_data_account_info(
    rpc_client: &RpcClient,
    staking_account: &str,
) -> Result<StakingDataAccount, String> {
    let result = rpc_client.get_account(&staking_account.parse::<Pubkey>().unwrap());
    match result {
        Ok(account) => match StakingData::try_from_slice(account.data.as_slice()) {
            Ok(staking_data) => Ok(StakingDataAccount::new(
                staking_account.to_string(),
                staking_data,
            )),
            Err(error) => {
                info!(
                    "Failed staking_account: {:?} : {:?}",
                    staking_account, account.data
                );
                let err = format!("StakingData parsing error! {:?}", error);
                warn!("{}", err);
                Err(err)
            }
        },
        Err(error) => {
            info!("Failed staking_account: {:?}", staking_account);
            let err = format!("StakingData {} Client error: {:?}", staking_account, error);
            warn!("{}", err);
            Err(err)
        }
    }
}

pub fn get_wallet_mint_balance(
    rpc_client: &RpcClient,
    account_key: &str,
    mint_key: &str,
) -> Result<f64, String> {
    let wallet_ata =
        rpc_wrapper::get_associated_account(rpc_client, &account_key, &account_key, &mint_key).0;
    get_mint_balance(&rpc_client, &wallet_ata)
}

//funding wallet balance >= total staked + interest for next 3 days + buffer
pub fn get_funding_amount_required(
    total_staked: u64,
    unminted_interest: u64,
    buffer: u64,
    holding_wallet_gari_balance: f64,
) -> u64 {
    let base: u64 = 10;
    let decimals: u64 = base.pow(9 as u32);
    let wallet_balance_u64 = (holding_wallet_gari_balance * decimals as f64) as u64;
    let total = total_staked + unminted_interest + buffer;
    if total > wallet_balance_u64 {
        return total - wallet_balance_u64;
    }
    return 0;
}
