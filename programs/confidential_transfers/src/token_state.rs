use anchor_lang::prelude::*;
use anchor_spl::token_interface::spl_token_2022::{
    extension::{transfer_fee::TransferFeeConfig, BaseStateWithExtensions, StateWithExtensions},
    state::Mint as MintState,
};

use crate::error::StablecoinError;

pub fn mint_decimals(mint: &AccountInfo) -> Result<u8> {
    let data = mint.try_borrow_data()?;
    let mint = StateWithExtensions::<MintState>::unpack(&data)?;

    Ok(mint.base.decimals)
}

pub fn mint_decimals_and_epoch_fee(mint: &AccountInfo, amount: u64) -> Result<(u8, u64)> {
    let data = mint.try_borrow_data()?;
    let mint = StateWithExtensions::<MintState>::unpack(&data)?;
    let fee_config = mint
        .get_extension::<TransferFeeConfig>()
        .map_err(|_| error!(StablecoinError::MissingTransferFeeConfig))?;
    let fee = fee_config
        .calculate_epoch_fee(Clock::get()?.epoch, amount)
        .ok_or(StablecoinError::FeeCalculationOverflow)?;

    Ok((mint.base.decimals, fee))
}
