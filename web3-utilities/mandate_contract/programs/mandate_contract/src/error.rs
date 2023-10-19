use anchor_lang::prelude::*;

#[error_code]
pub enum MandateError {
    #[msg("Given user token account amount lesser than mandate amount")]
    InsufficientUserTokenATAAmount,
    #[msg("Given mandate amount is lesser than platform minimum mandate amount")]
    InvalidMandateAmount,
    #[msg("Given validity is lesser than platform minimum mandate valifity")]
    InvalidValitity,
    #[msg("Given max transaction amount is greater than platform max transaction amount")]
    InvalidMaxTxnAmount,
    #[msg("Math Error")]
    MathError,
    #[msg("Given user has already revoked")]
    UserRevokedAlready,
}
