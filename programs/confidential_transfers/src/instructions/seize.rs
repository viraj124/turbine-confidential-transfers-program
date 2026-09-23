use anchor_lang::prelude::*;
use anchor_spl::token_interface::{transfer_checked_with_fee, Token2022, TransferCheckedWithFee};

use crate::token_state::mint_decimals_and_epoch_fee;

#[derive(Accounts)]
pub struct Seize<'info> {
    /// CHECK: validated by Token-2022 during the transfer.
    #[account(mut, owner = token_program.key())]
    pub source: UncheckedAccount<'info>,

    /// CHECK: ownership enforced here, contents parsed with
    /// `StateWithExtensions` for the fee.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022 during the transfer.
    #[account(mut, owner = token_program.key())]
    pub destination: UncheckedAccount<'info>,

    pub permanent_delegate: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
}

impl<'info> Seize<'info> {
    pub fn seize(&self, amount: u64) -> Result<()> {
        let (decimals, fee) = mint_decimals_and_epoch_fee(&self.mint, amount)?;

        transfer_checked_with_fee(
            CpiContext::new(
                self.token_program.key(),
                TransferCheckedWithFee {
                    token_program_id: self.token_program.to_account_info(),
                    source: self.source.to_account_info(),
                    mint: self.mint.to_account_info(),
                    destination: self.destination.to_account_info(),
                    authority: self.permanent_delegate.to_account_info(),
                },
            ),
            amount,
            decimals,
            fee,
        )?;

        msg!("seized {} from {}", amount, self.source.key());
        Ok(())
    }
}
