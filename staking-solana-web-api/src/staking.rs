use rocket::serde::Deserialize;

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct StakingConfig {
    pub on_chain_endpoint: String,
    pub staking_program_address: String,
    pub staking_account_address: String,
    pub staking_account_token_mint: String,
    pub staking_holding_wallet_owner: String,
    pub solana_web_api_rust_log: String,
    pub fee_payer_address: String,
    pub fee_payer_private_key: String,
    pub send_transaction_simulate: bool,
    pub staking_holding_wallet: String,
    pub funding_wallet_buffer: String,
    pub interest_buffer: String,
    pub funding_wallet_private_key: String,
    pub funding_wallet_address: String,
}

pub mod instruction {
    pub const STAKE: [u8; 8] = [206, 176, 202, 18, 200, 209, 179, 108];
    pub const UNSTAKE: [u8; 8] = [90, 95, 107, 42, 205, 124, 50, 225];
    pub const INIT_STAKING_USER: [u8; 8] = [234, 231, 125, 196, 142, 56, 154, 234];
    pub const ACCRUE_INTEREST: [u8; 8] = [47, 40, 115, 198, 91, 12, 222, 49];
    pub const FUND_STAKING_WALLET: [u8; 8] = [100, 89, 7, 204, 201, 208, 122, 186];
}
