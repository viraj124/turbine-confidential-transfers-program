use anchor_lang::{prelude::*, solana_program::program::invoke};
use anchor_spl::token_interface::{
    spl_token_2022::extension::confidential_transfer::instruction::deposit, Token2022,
};

use crate::token_state::mint_decimals;

#[derive(Accounts)]
pub struct DepositConfidential<'info> {
    /// CHECK: validated by Token-2022, which requires it to be configured,
    /// approved and not frozen.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    /// CHECK: ownership enforced here, contents parsed with
    /// `StateWithExtensions` for the decimals.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    pub owner: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
}

impl<'info> DepositConfidential<'info> {
    pub fn deposit_confidential(&self, amount: u64) -> Result<()> {
        let decimals = mint_decimals(&self.mint)?;

        let ix = deposit(
            &self.token_program.key(),
            &self.token_account.key(),
            &self.mint.key(),
            amount,
            decimals,
            &self.owner.key(),
            &[],
        )?;
        invoke(
            &ix,
            &[
                self.token_account.to_account_info(),
                self.mint.to_account_info(),
                self.owner.to_account_info(),
                self.token_program.to_account_info(),
            ],
        )?;

        msg!("deposited {} into the confidential balance", amount);
        Ok(())
    }
}
