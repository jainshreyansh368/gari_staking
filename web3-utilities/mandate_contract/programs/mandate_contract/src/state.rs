use anchor_lang::prelude::*;

#[derive(Debug, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[account]
pub struct PlatformData {
    pub is_initialized: bool,
    pub admin: Pubkey,
    pub min_mandate_amount: u64,
    pub min_validity: i64,
    pub max_tx_amount: u64,
    pub min_charge_period: i64,
}

impl PlatformData {
    pub const LEN: usize = 73;
}

#[derive(Debug, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[account]
pub struct GariTreasuryState {
    pub is_initialized: bool,
    pub treasury_account: Pubkey,
}

impl GariTreasuryState {
    pub const LEN: usize = 33;
}

#[derive(Debug, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[account]
pub struct UserMandateData {
    pub is_initialized: bool,
    pub user: Pubkey,
    pub user_token_account: Pubkey,
    pub approved_amount: u64,
    pub amount_transfered: u64,
    pub amount_per_transaction: u64,
    pub mandate_validity: i64,
    pub last_charge_time: i64,
    pub next_charge_time: i64,
    pub revoked: bool,
}

impl UserMandateData {
    pub const LEN: usize = 98 + 8 + 8;
}
