use borsh::BorshDeserialize;
use log::{info, warn};
use rocket::{
    serde::{Deserialize, Serialize},
    tokio::time::sleep,
};
use solana_client::rpc_client::{GetConfirmedSignaturesForAddress2Config, RpcClient};
use solana_program::pubkey::Pubkey;
use solana_sdk::{
    commitment_config::CommitmentConfig, instruction::Instruction, signature::Signature,
};
use solana_transaction_status::option_serializer::OptionSerializer;
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiMessage,
    UiTransactionEncoding, UiTransactionTokenBalance,
};
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::Duration;

pub fn get_signatures(
    rpc_client: &RpcClient,
    address: &str,
    limit: Option<usize>,
    before: Option<&str>,
    until: Option<&str>,
) -> Option<Vec<String>> {
    let before = match before {
        Some(b) => match b.parse::<Signature>() {
            Ok(b) => Some(b),
            Err(_) => None,
        },
        None => None,
    };
    let until = match until {
        Some(u) => match u.parse::<Signature>() {
            Ok(u) => Some(u),
            Err(_) => None,
        },
        None => None,
    };
    let config = GetConfirmedSignaturesForAddress2Config {
        limit,
        before,
        until,
        commitment: Some(CommitmentConfig::confirmed()),
    };
    let pubkey = address.parse::<Pubkey>().unwrap();
    let signatures = rpc_client.get_signatures_for_address_with_config(&pubkey, config);
    match signatures {
        Ok(sig) => {
            if sig.len() == 0 {
                None
            } else {
                Some(sig.iter().map(|s| s.signature.to_owned()).collect())
            }
        }
        Err(err) => {
            warn!("Filtering signature error: {:?}", err);
            None
        }
    }
}

pub async fn get_user_data_account_with_transactions(
    rpc_client: &RpcClient,
    staking_account_address: &str,
    fee_payer_address: &str,
    signatures: Vec<String>,
    non_parsed_transactions_set: &mut HashSet<String>,
    non_parsed_transactions: &mut Vec<String>,
    to_retry: bool,
) -> HashMap<String, StakingUserDataAccount> {
    info!("get transactions started: {:?}", signatures.len());
    let mut staking_user_data_accounts: HashMap<String, StakingUserDataAccount> = HashMap::new();
    for sign in signatures {
        let signature = match sign.parse::<Signature>() {
            Ok(s) => s,
            Err(error) => {
                warn!("{:?} failed to parse : {:?}", sign, error);
                continue;
            }
        };
        let mut result: Option<EncodedConfirmedTransactionWithStatusMeta> = None;
        let mut retry = 0;
        let mut sleep_time = 15;
        while retry < 5 {
            let client_result = rpc_client.get_transaction(&signature, UiTransactionEncoding::Json);
            match client_result {
                Ok(r) => {
                    result = Some(r);
                    break;
                }
                Err(ref error) => {
                    warn!(
                        "{:?} parsing failed for signature {} -- Type: {:?}",
                        error.request.unwrap(),
                        signature,
                        error.kind,
                    );
                    if !to_retry {
                        break;
                    }
                    info!("Sleeping {} seconds", sleep_time);
                    sleep(Duration::from_secs(sleep_time)).await;
                    info!("Awakening after {} seconds", sleep_time);
                    retry += 1;
                    sleep_time *= 2;
                }
            };
        }
        if result.is_none() {
            warn!("Failed to parse transaction {}, 3 times", sign);
            if non_parsed_transactions_set.insert(sign.to_owned()) {
                non_parsed_transactions.push(sign.to_owned());
            }
            continue;
        }
        let result = result.unwrap();

        let meta = match result.transaction.meta {
            Some(meta) => meta,
            None => {
                warn!("meta parsing failed");
                break;
            }
        };

        let log_messages = match meta.log_messages {
            OptionSerializer::Some(log) => log,
            _ => {
                warn!("Empty logs");
                vec![]
            }
        };

        let mut instruction_type: Option<&str> = None;
        let is_init_user_instruction =
            log_messages.len() > 9 && log_messages[4].ends_with(&INIT_USER_INSTRUCTION);
        if log_messages[2].ends_with(&STAKE_INSTRUCTION)
            || (is_init_user_instruction && log_messages[9].ends_with(&STAKE_INSTRUCTION))
        {
            instruction_type = Some(STAKE);
        } else if log_messages[2].ends_with(&UNSTAKE_INSTRUCTION)
            || (is_init_user_instruction && log_messages[9].ends_with(&UNSTAKE_INSTRUCTION))
        {
            instruction_type = Some(UNSTAKE);
        }

        if instruction_type.is_none() {
            continue;
        }

        let instruction_type = instruction_type.unwrap();

        let block_time = result.block_time.unwrap();
        let transaction = result.transaction.transaction;
        let message = match transaction {
            EncodedTransaction::Json(transaction) => transaction.message,
            _ => {
                info!("{:?}", transaction);
                warn!("EncodedTransaction parsing failed!");
                break;
            }
        };
        let message = match message {
            UiMessage::Raw(message) => message,
            _ => {
                info!("{:?}", message);
                warn!("UiMessage parsing failed!");
                break;
            }
        };
        let instruction_index = if is_init_user_instruction { 1 } else { 0 };
        let is_gari_user = message.account_keys[0].eq(fee_payer_address);
        let index0 = message.instructions[instruction_index].accounts[0] as usize;
        let index1 = message.instructions[instruction_index].accounts[1] as usize;
        let index2 = message.instructions[instruction_index].accounts[2] as usize;
        let index3 = message.instructions[instruction_index].accounts[3] as usize;

        let staking_data_account = message.account_keys[index3].to_owned();

        if !staking_account_address.eq(&staking_data_account) {
            trace!("dropping signature: {:?}", signature.to_string());
            continue;
        }

        let staking_user_data_account = message.account_keys[index0].to_owned();
        let user_spl_token_owner = message.account_keys[index2].to_owned();

        let pre_balance: u64 = get_balance(meta.pre_token_balances, &user_spl_token_owner);
        let post_balance: u64 = get_balance(meta.post_token_balances, &user_spl_token_owner);

        let amount = if STAKE.eq(instruction_type) {
            pre_balance - post_balance
        } else {
            post_balance - pre_balance
        };

        let transaction = Transaction::new(
            block_time,
            meta.err.is_some(),
            instruction_type.to_string(),
            staking_data_account,
            sign.to_owned(),
            amount,
        );
        let user_account = staking_user_data_accounts.get_mut(&user_spl_token_owner);
        let user_token_wallet = message.account_keys[index1].to_owned();
        match user_account {
            Some(user_account) => {
                user_account.transactions.push(transaction);
                if is_gari_user && !user_account.is_gari_user {
                    user_account.is_gari_user = is_gari_user;
                }
                if user_account.user_token_wallet.is_empty() {
                    user_account.user_token_wallet = user_token_wallet
                }
            }
            None => {
                let account = StakingUserDataAccount::new(
                    staking_user_data_account.to_owned(),
                    user_token_wallet,
                    vec![transaction],
                    is_gari_user,
                );
                staking_user_data_accounts.insert(user_spl_token_owner, account);
            }
        }
    }
    info!("get transactions completed");
    staking_user_data_accounts
}

fn get_balance(
    opt_token_balances: OptionSerializer<Vec<UiTransactionTokenBalance>>,
    user_spl_token_owner: &str,
) -> u64 {
    let mut balance: u64 = 0;
    match opt_token_balances {
        OptionSerializer::Some(token_balances) => {
            for token_balance in token_balances {
                match token_balance.owner {
                    OptionSerializer::Some(owner) => {
                        if user_spl_token_owner.eq(&owner) {
                            let val =
                                u64::from_str_radix(&token_balance.ui_token_amount.amount, 10);
                            balance = match val {
                                Ok(b) => b,
                                Err(_) => 0,
                            }
                        }
                    }
                    _ => warn!("No token_balance owner found!"),
                }
            }
        }
        _ => warn!("No token_balances found!"),
    }
    balance
}

pub async fn get_staking_user_account_info(
    rpc_client: &RpcClient,
    user_transactions: &mut HashMap<String, StakingUserDataAccount>,
) {
    let user_transactions_len = user_transactions.len();
    info!(
        "get_staking_user_account_info started: {:?}",
        user_transactions_len
    );

    let mut user_data_account_pubkeys: Vec<Pubkey> = vec![];
    let mut count = 0;
    let mut account_token_map: HashMap<String, String> = HashMap::new();
    let mut wallet_token_map: HashMap<String, String> = HashMap::new();
    let user_spl_tokens: Vec<String> = user_transactions.iter().map(|t| t.0.to_owned()).collect();
    for user_spl_token_owner in user_spl_tokens {
        let staking_user_data_account = &user_transactions
            .get(&user_spl_token_owner)
            .unwrap()
            .staking_user_data_account
            .to_owned();
        account_token_map.insert(
            staking_user_data_account.to_owned(),
            user_spl_token_owner.to_owned(),
        );
        let staking_user_data_account = staking_user_data_account.parse::<Pubkey>().unwrap();
        let user_token_wallet = &user_transactions
            .get(&user_spl_token_owner)
            .unwrap()
            .user_token_wallet;
        wallet_token_map.insert(user_token_wallet.to_owned(), user_spl_token_owner);
        user_data_account_pubkeys.push(staking_user_data_account);
        count += 1;

        if user_data_account_pubkeys.len() == 100 || count == user_transactions_len {
            get_multiple_accounts(
                rpc_client,
                &user_data_account_pubkeys,
                user_transactions,
                &wallet_token_map,
                &account_token_map,
            );
            /*get_account(
                rpc_client,
                &user_data_account_pubkeys,
                user_transactions,
                &account_token_map,
            )
            .await;*/
            user_data_account_pubkeys.clear();
        }
    }

    info!("get_staking_user_account_info completed");
}

// temporary fix
#[allow(dead_code)]
async fn get_account(
    rpc_client: &RpcClient,
    user_data_account_pubkeys: &Vec<Pubkey>,
    user_transactions: &mut HashMap<String, StakingUserDataAccount>,
    account_token_map: &HashMap<String, String>,
) {
    let mut count = 0;
    for user_account_pubkey in user_data_account_pubkeys {
        count += 1;
        let account_result = rpc_client.get_account(user_account_pubkey);

        match account_result {
            Ok(account) => {
                let data = match StakingUserData::try_from_slice(account.data.as_slice()) {
                    Ok(data) => data,
                    Err(error) => {
                        warn!("Parsing StakingUserData error: {:?}", error);
                        continue;
                    }
                };
                let user_account_pubkey_str = user_account_pubkey.to_string();
                if !account_token_map.contains_key(&user_account_pubkey_str) {
                    continue;
                }
                let user_spl_token_owner = account_token_map.get(&user_account_pubkey_str).unwrap();

                let temp: &StakingUserDataAccount =
                    user_transactions.get(user_spl_token_owner).unwrap();
                let mut user_data_account = StakingUserDataAccount::new(
                    temp.staking_user_data_account.to_owned(),
                    data.user_token_wallet.to_string(),
                    (*temp.transactions).to_vec(),
                    temp.is_gari_user,
                );
                user_data_account.set_details(
                    data.ownership_share,
                    data.staked_amount,
                    data.locked_amount,
                    data.locked_until,
                    data.last_staking_timestamp,
                );
                user_transactions.insert(user_spl_token_owner.to_owned(), user_data_account);
            }
            Err(error) => {
                warn!("Error fetching multiple accounts: {:?}", error);
            }
        };
        if count % 100 == 0 {
            info!("sleeping after {}", count);
            sleep(Duration::from_secs(1)).await;
        }
    }
}

fn get_multiple_accounts(
    rpc_client: &RpcClient,
    user_data_account_pubkeys: &Vec<Pubkey>,
    user_transactions: &mut HashMap<String, StakingUserDataAccount>,
    wallet_token_map: &HashMap<String, String>,
    account_token_map: &HashMap<String, String>,
) {
    let account_result = rpc_client.get_multiple_accounts(&user_data_account_pubkeys);

    let mut account_not_found = 0;
    match account_result {
        Ok(acounts) => {
            let account_len = acounts.len();
            for (index, account) in acounts.iter().enumerate() {
                match account {
                    Some(account) => {
                        let data = match StakingUserData::try_from_slice(account.data.as_slice()) {
                            Ok(data) => data,
                            Err(error) => {
                                warn!("Parsing StakingUserData error: {:?}", error);
                                continue;
                            }
                        };
                        let user_token_wallet = data.user_token_wallet.to_string();

                        let user_data_account_pubkey = user_data_account_pubkeys.get(index);
                        let user_spl_token_owner_1 = if user_data_account_pubkey.is_some() {
                            account_token_map
                                .get(&user_data_account_pubkey.unwrap().to_string())
                                .unwrap_or(&"".to_owned())
                                .to_owned()
                        } else {
                            "".to_owned()
                        };

                        let user_spl_token_owner_2 = wallet_token_map
                            .get(&user_token_wallet)
                            .unwrap_or(&"".to_owned())
                            .to_owned();

                        if user_spl_token_owner_1.is_empty() && user_spl_token_owner_2.is_empty() {
                            warn!("user_spl_token_owner not found");
                            continue;
                        }

                        let user_spl_token_owner = if user_spl_token_owner_1.is_empty() {
                            user_spl_token_owner_2
                        } else {
                            user_spl_token_owner_1
                        };

                        let temp: &StakingUserDataAccount =
                            user_transactions.get(&user_spl_token_owner).unwrap();
                        let mut user_data_account = StakingUserDataAccount::new(
                            temp.staking_user_data_account.to_owned(),
                            user_token_wallet,
                            (*temp.transactions).to_vec(),
                            temp.is_gari_user,
                        );
                        user_data_account.set_details(
                            data.ownership_share,
                            data.staked_amount,
                            data.locked_amount,
                            data.locked_until,
                            data.last_staking_timestamp,
                        );
                        user_transactions
                            .insert(user_spl_token_owner.to_owned(), user_data_account);
                    }
                    None => account_not_found += 1,
                }
            }
            if account_not_found > 0 {
                warn!(
                    "Total accounts: {} || Accounts not found: {}",
                    account_len, account_not_found
                );
            }
        }
        Err(error) => {
            warn!("Error fetching multiple accounts: {:?}", error);
        }
    };
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

pub fn get_associated_account(
    rpc_client: &RpcClient,
    user_spl_token_owner: &str,
    fee_payer: &str,
    token_mint: &str,
) -> (Pubkey, Option<Instruction>) {
    let user_spl_token_owner = user_spl_token_owner.parse::<Pubkey>().unwrap();
    let fee_payer = fee_payer.parse::<Pubkey>().unwrap();
    let token_mint = token_mint.parse::<Pubkey>().unwrap();

    let associated_token_account = spl_associated_token_account::get_associated_token_address(
        &user_spl_token_owner,
        &token_mint,
    );
    //This to check if account exists
    if let Ok(_account) = rpc_client.get_account(&associated_token_account) {
        (associated_token_account, None)
    } else {
        //create and send
        let instruction =
            spl_associated_token_account::instruction::create_associated_token_account(
                &fee_payer,
                &user_spl_token_owner,
                &token_mint,
                &spl_token::id(),
            );
        //associated_token_account will always be same pubkey
        //Add this instruction as first step, then init stake and then stake
        (associated_token_account, Some(instruction))
    }
}

pub fn is_staking_user_account_initialized(
    rpc_client: &RpcClient,
    staking_user_data_account: &Pubkey,
) -> bool {
    if let Ok(_user_account) = rpc_client.get_account(staking_user_data_account) {
        true
    } else {
        false
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct Transaction {
    block_time: i64,
    error: bool,
    instruction_type: String,
    staking_data_account: String,
    transaction_signature: String,
    amount: u64,
}

const STAKE_INSTRUCTION: &str = "Instruction: stake";
const STAKE: &str = "stake";
const UNSTAKE_INSTRUCTION: &str = "Instruction: unstake";
const UNSTAKE: &str = "unstake";
const INIT_USER_INSTRUCTION: &str = "Instruction: initialize staking user";

impl Transaction {
    fn new(
        block_time: i64,
        error: bool,
        instruction_type: String,
        staking_data_account: String,
        transaction_signature: String,
        amount: u64,
    ) -> Transaction {
        Transaction {
            block_time,
            error,
            instruction_type,
            staking_data_account,
            transaction_signature,
            amount,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct StakingUserDataAccount {
    staking_user_data_account: String,
    user_token_wallet: String,
    transactions: Vec<Transaction>,
    is_gari_user: bool,
    ownership_share: u128,
    staked_amount: u64,
    locked_amount: u64,
    locked_until: i64,
    last_staking_timestamp: i64,
}

impl StakingUserDataAccount {
    pub fn new(
        staking_user_data_account: String,
        user_token_wallet: String,
        transactions: Vec<Transaction>,
        is_gari_user: bool,
    ) -> StakingUserDataAccount {
        StakingUserDataAccount {
            staking_user_data_account: staking_user_data_account,
            user_token_wallet: user_token_wallet,
            transactions: transactions,
            is_gari_user,
            ownership_share: 0,
            staked_amount: 0,
            locked_amount: 0,
            locked_until: 0,
            last_staking_timestamp: 0,
        }
    }

    fn set_details(
        &mut self,
        ownership_share: u128,
        staked_amount: u64,
        locked_amount: u64,
        locked_until: i64,
        last_staking_timestamp: i64,
    ) {
        self.ownership_share = ownership_share;
        self.staked_amount = staked_amount;
        self.locked_amount = locked_amount;
        self.locked_until = locked_until;
        self.last_staking_timestamp = last_staking_timestamp;
    }
}

#[derive(BorshDeserialize, PartialEq, Debug)]
struct StakingUserData {
    program_accounts: u64,
    /// User wallet for holding staking token
    user_token_wallet: Pubkey,
    /// Link to staking pool
    staking_data: Pubkey,
    ownership_share: u128,
    staked_amount: u64,
    /// Amount of shares locked for the governance proposal vote
    locked_amount: u64,
    locked_until: i64,
    last_staking_timestamp: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct StakingDataAccount {
    pub staking_data_account: String,
    pub owner: String,
    pub staking_account_token: String,
    pub holding_wallet: String,
    pub holding_bump: u8,
    pub total_staked: u64,
    pub total_shares: u128,
    pub interest_rate_hourly: u16,
    pub est_apy: u16,
    pub max_interest_rate_hourly: u16,
    pub last_interest_accrued_timestamp: i64,
    pub minimum_staking_amount: u64,
    pub minimum_staking_period_sec: u32,
    pub is_interest_accrual_paused: bool,
    pub is_active: bool,
}

impl StakingDataAccount {
    fn new(staking_data_account: String, account: StakingData) -> StakingDataAccount {
        StakingDataAccount {
            staking_data_account: staking_data_account,
            owner: account.owner.to_string(),
            staking_account_token: account.staking_token.to_string(),
            holding_wallet: account.holding_wallet.to_string(),
            holding_bump: account.holding_bump,
            total_staked: account.total_staked,
            total_shares: account.total_shares,
            interest_rate_hourly: account.interest_rate_hourly,
            est_apy: Self::calculate_est_apy(account.interest_rate_hourly),
            max_interest_rate_hourly: account.max_interest_rate_hourly,
            last_interest_accrued_timestamp: account.last_interest_accrued_timestamp,
            minimum_staking_amount: account.minimum_staking_amount,
            minimum_staking_period_sec: account.minimum_staking_period_sec,
            is_interest_accrual_paused: account.is_interest_accrual_paused,
            is_active: true,
        }
    }

    pub fn calculate_est_apy(apr: u16) -> u16 {
        let ten_pow = 100_000_000.0;
        let nop = 8760.0;
        let apy = ((((ten_pow + apr as f64) / ten_pow).powf(nop)) * 10000.0) - 10000.0;
        let apy = apy.round() as u16;
        info!("apr: {:?} | apy: {:?}", apr, apy);
        apy
    }
}

#[derive(BorshDeserialize, PartialEq, Debug)]
pub struct StakingData {
    program_accounts: u64,
    /// Staking pool owner
    owner: Pubkey,
    /// Staking token
    staking_token: Pubkey,
    /// Wallet for storing staking token
    holding_wallet: Pubkey,
    /// PDA bump for holding wallet (needs for signatures)
    holding_bump: u8,
    total_staked: u64,
    total_shares: u128,
    /// Hourly interest rate in 1e-8 (1/10000 of a basis point)
    interest_rate_hourly: u16,
    max_interest_rate_hourly: u16,
    last_interest_accrued_timestamp: i64,
    minimum_staking_amount: u64,
    minimum_staking_period_sec: u32,
    is_interest_accrual_paused: bool,
}
