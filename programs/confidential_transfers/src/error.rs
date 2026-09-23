use anchor_lang::error_code;

#[error_code]
pub enum StablecoinError {
    #[msg("mint has no transfer fee config")]
    MissingTransferFeeConfig,
    #[msg("transfer fee calculation overflowed")]
    FeeCalculationOverflow,
}
