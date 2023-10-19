use sea_orm::DbBackend;

pub const DB_BACKEND: DbBackend = DbBackend::Postgres;

pub const SUBSEQUENT_STAKING: &str = r#"SELECT s1.instruction_type, s1.user_spl_token_owner, s1.block_time , s1.amount
	FROM staking_user_transaction_history s1, (
	SELECT count(*) AS cnt, user_spl_token_owner
		FROM staking_user_transaction_history
		WHERE error = false AND amount > 0
		GROUP BY user_spl_token_owner) s2
	WHERE s1.error = false AND s1.user_spl_token_owner = s2.user_spl_token_owner
	AND s1.amount > 0 AND s2.cnt > 1
	ORDER BY s1.user_spl_token_owner ASC, s1.block_time ASC
	OFFSET $1 ROWS LIMIT $2"#;

pub const USER_TRANSACTIONS: &str = r#"SELECT * FROM staking_user_transaction_history
	WHERE error = false AND user_spl_token_owner IN 
		(SELECT user_spl_token_owner FROM staking_user_transaction_history
	    WHERE error = false AND block_time >= $1 AND block_time < $2)
    ORDER BY block_time ASC"#;

pub const TOTAL_AMOUNT_WITHDRAWN: &str = r#"SELECT SUM(amount_withdrawn) AS total_amount_withdrawn 
	FROM staking_user_transaction_history
	WHERE error = false AND instruction_type = 'unstake' AND amount_withdrawn IS NOT NULL AND user_spl_token_owner = $1
    GROUP BY user_spl_token_owner"#;
