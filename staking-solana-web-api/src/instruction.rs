use borsh::BorshSerialize;
use solana_program::{system_program, sysvar::rent};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

pub fn get_init(
    program_id: Pubkey,
    instruction_data: &InstructionDataInit,
    staking_user_data_account: Pubkey,
    user_spl_token_account: Pubkey,
    user_spl_token_owner: &str,
    fee_payer: &str,
    staking_account_address: &str,
    staking_account_token_mint: &str,
) -> Instruction {
    Instruction::new_with_borsh(
        program_id,
        instruction_data,
        vec![
            // 0. `[writable]` UserStakingData account
            // db / find_program_address
            AccountMeta::new(staking_user_data_account, false),
            // 1. `[]` User Token Wallet (SPL Token Account)
            AccountMeta::new_readonly(user_spl_token_account, false),
            // 2. `[]` User Token Wallet owner
            AccountMeta::new_readonly(user_spl_token_owner.parse::<Pubkey>().unwrap(), false),
            // 3. `[writable, signer]` Fee Payer
            AccountMeta::new(fee_payer.parse::<Pubkey>().unwrap(), true),
            // 4. `[]` Staking data account
            AccountMeta::new_readonly(staking_account_address.parse::<Pubkey>().unwrap(), false),
            // 5. `[]` Staking token Mint
            // config
            AccountMeta::new_readonly(staking_account_token_mint.parse::<Pubkey>().unwrap(), false),
            // 6. `[]` System program
            AccountMeta::new_readonly(system_program::id(), false),
            // 7. `[]` Rent sysvar
            AccountMeta::new_readonly(rent::id(), false),
        ],
    )
}

pub fn get_stake(
    program_id: Pubkey,
    instruction_data: &InstructionDataWithAmount,
    staking_user_data_account: Pubkey,
    user_spl_token_account: Pubkey,
    user_spl_token_owner: &str,
    staking_account_address: &str,
    staking_holding_wallet: &str,
    staking_account_token_mint: &str,
) -> Instruction {
    Instruction::new_with_borsh(
        program_id,
        instruction_data,
        vec![
            // 0. `[writable]` StakingUserData account
            // db / find_program_address
            AccountMeta::new(staking_user_data_account, false),
            // 1. `[writable]` User SPL Token account
            // db / get_associated_account
            AccountMeta::new(user_spl_token_account, false),
            // 2. `[signer]` User SPL Token owner
            AccountMeta::new(user_spl_token_owner.parse::<Pubkey>().unwrap(), true),
            // 3. `[writable]` StakingData account
            // config
            AccountMeta::new(staking_account_address.parse::<Pubkey>().unwrap(), false),
            // 4. `[writable]` StakingHoldingWallet account
            // db staking_data holding_wallet
            AccountMeta::new(staking_holding_wallet.parse::<Pubkey>().unwrap(), false),
            // 5. `[writable]` Stake token Mint
            // config
            AccountMeta::new(staking_account_token_mint.parse::<Pubkey>().unwrap(), false),
            // 6. `[]` SPL token program account
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
    )
}

pub fn get_unstake(
    program_id: Pubkey,
    instruction_data: &InstructionDataWithAmount,
    staking_user_data_account: Pubkey,
    user_spl_token_account: Pubkey,
    user_spl_token_owner: &str,
    staking_account_address: &str,
    staking_holding_wallet: &str,
    staking_holding_wallet_owner: &str,
    staking_account_token_mint: &str,
) -> Instruction {
    Instruction::new_with_borsh(
        program_id,
        instruction_data,
        vec![
            // 0. `[writable]` StakingUserData account
            // db / find_program_address
            AccountMeta::new(staking_user_data_account, false),
            // 1. `[writable]` User SPL Token account
            // db / get_associated_account
            AccountMeta::new(user_spl_token_account, false),
            // 2. `[signer]` User SPL Token owner
            AccountMeta::new(user_spl_token_owner.parse::<Pubkey>().unwrap(), true),
            // 3. `[writable]` StakingData account
            // config
            AccountMeta::new(staking_account_address.parse::<Pubkey>().unwrap(), false),
            // 4. `[writable]` StakingHoldingWallet account
            // db staking_data holding_wallet
            AccountMeta::new(staking_holding_wallet.parse::<Pubkey>().unwrap(), false),
            // 5. `[]`  StakingHoldingWallet owner (pda of [staking_program_id, staking_data])
            AccountMeta::new_readonly(
                staking_holding_wallet_owner.parse::<Pubkey>().unwrap(),
                false,
            ),
            // 6. `[writable]` Stake token Mint
            // config
            AccountMeta::new(staking_account_token_mint.parse::<Pubkey>().unwrap(), false),
            // 7. `[]` SPL token program account
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
    )
}

pub fn get_accrued_interest(
    program_id: Pubkey,
    instruction_data: &InstructionAccrueInterest,
    staking_account_address: &str,
    staking_holding_wallet: &str,
) -> Instruction {
    Instruction::new_with_borsh(
        program_id,
        instruction_data,
        vec![
            // 0. `[writable]` StakingData account
            // db / find_program_address
            AccountMeta::new(staking_account_address.parse::<Pubkey>().unwrap(), false),
            // 1. `[]` StakingHoldingWallet account
            // db / get_associated_account
            AccountMeta::new(staking_holding_wallet.parse::<Pubkey>().unwrap(), false),
        ],
    )
}

pub fn get_fund_staking(
    program_id: Pubkey,
    instruction_data: &InstructionFundStaking,
    staking_account_address: &str,
    user_spl_token_account: Pubkey,
    user_spl_token_owner: &str,
    staking_holding_wallet: &str,
    staking_account_token_mint: &str,
) -> Instruction {
    Instruction::new_with_borsh(
        program_id,
        instruction_data,
        vec![
            // 0. `[writable]` StakingData account
            AccountMeta::new(staking_account_address.parse::<Pubkey>().unwrap(), false),
            // 1. `[writable]` User SPL Token account
            AccountMeta::new(user_spl_token_account, false),
            // 2. `[signer]` User SPL Token owner
            AccountMeta::new(user_spl_token_owner.parse::<Pubkey>().unwrap(), true),
            // 3. `[writable]` StakingHoldingWallet account
            AccountMeta::new(staking_holding_wallet.parse::<Pubkey>().unwrap(), false),
            // 4. `[writable]` Stake token Mint
            AccountMeta::new(staking_account_token_mint.parse::<Pubkey>().unwrap(), false),
            // 5. `[]` SPL token program account
            AccountMeta::new_readonly(spl_token::id(), false),
        ],
    )
}

#[derive(Debug, BorshSerialize)]
pub struct InstructionDataWithAmount {
    pub instruction_type: [u8; 8],
    pub amount: u64,
}

#[derive(Debug, BorshSerialize)]
pub struct InstructionDataInit {
    pub instruction_type: [u8; 8],
}

#[derive(Debug, BorshSerialize)]
pub struct InstructionAccrueInterest {
    pub instruction_type: [u8; 8],
}

#[derive(Debug, BorshSerialize)]
pub struct InstructionFundStaking {
    pub instruction_type: [u8; 8],
    pub amount: u128,
}
pub const SOL_ADDRESS: &str = "11111111111111111111111111111111";
