use sea_orm::DbBackend;

pub const DB_BACKEND: DbBackend = DbBackend::Postgres;

pub const USER_HISTORY: &str = r#"SELECT staking_user_transaction_history.transaction_signature,
    staking_user_transaction_history.block_time,
    staking_user_transaction_history.error,
    staking_user_transaction_history.instruction_type,
    staking_user_transaction_history.staking_data_account,
    staking_user_transaction_history.staking_user_data_account,
    staking_user_transaction_history.user_spl_token_owner,
    staking_user_transaction_history.amount,
    staking_user_transaction_history.amount_withdrawn
    FROM staking_user_transaction_history
    WHERE user_spl_token_owner = $1 AND instruction_type = $2
    ORDER BY block_time DESC OFFSET $3 ROWS LIMIT $4"#;

pub const USER_HISTORY_COUNT: &str = r#"SELECT COUNT(*) AS total_records 
	FROM staking_user_transaction_history
    WHERE user_spl_token_owner = $1 AND instruction_type = $2"#;

pub const TOKEN_OWNER: &str = r#"SELECT S.* FROM
    (SELECT RANK() OVER (ORDER BY ownership_share DESC) user_rank,
    staking_user_data.user_spl_token_owner,
    staking_user_data.staking_user_data_account,
    staking_user_data.staking_data_account,
    staking_user_data.is_gari_user,
    staking_user_data.ownership_share,
    staking_user_data.staked_amount,
    staking_user_data.locked_amount,
    staking_user_data.locked_until,
    staking_user_data.last_staking_timestamp,
    staking_user_data.balance,
    staking_user_data.amount_withdrawn
    FROM staking_user_data
    ORDER BY ownership_share DESC) AS S
    WHERE S.user_spl_token_owner = $1
    AND S.staking_data_account = $2"#;

pub const USERS: &str = r#"SELECT RANK() OVER (ORDER BY ownership_share DESC) user_rank,
    staking_user_data.user_spl_token_owner,
    staking_user_data.staking_user_data_account,
    staking_user_data.staking_data_account,
    staking_user_data.is_gari_user,
    staking_user_data.ownership_share,
    staking_user_data.staked_amount,
    staking_user_data.locked_amount,
    staking_user_data.locked_until,
    staking_user_data.last_staking_timestamp,
    staking_user_data.balance,
    staking_user_data.amount_withdrawn
    FROM staking_user_data
    ORDER BY ownership_share DESC OFFSET $1 ROWS LIMIT $2"#;

pub const GARI_USERS: &str = r#"SELECT RANK() OVER (ORDER BY ownership_share DESC) user_rank,
    staking_user_data.user_spl_token_owner,
    staking_user_data.staking_user_data_account,
    staking_user_data.staking_data_account,
    staking_user_data.is_gari_user,
    staking_user_data.ownership_share,
    staking_user_data.staked_amount,
    staking_user_data.locked_amount,
    staking_user_data.locked_until,
    staking_user_data.last_staking_timestamp,
    staking_user_data.balance,
    staking_user_data.amount_withdrawn
    FROM staking_user_data
    WHERE staking_user_data.is_gari_user = $1
    ORDER BY ownership_share DESC OFFSET $2 ROWS LIMIT $3"#;
