use anchor_lang::{prelude::*, solana_program::program::invoke};
use anchor_spl::token_interface::{
    spl_token_2022::extension::confidential_transfer::{
        instruction::inner_withdraw, DecryptableBalance,
    },
    Token2022,
};
use spl_token_confidential_transfer_proof_extraction::instruction::ProofLocation;

use crate::token_state::mint_decimals;

#[derive(Accounts)]
pub struct WithdrawConfidential<'info> {
    /// CHECK: validated by Token-2022, which requires it to be configured and
    /// not frozen.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    /// CHECK: ownership enforced here, contents parsed with
    /// `StateWithExtensions` for the decimals.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    /// CHECK: context state account, verified by Token-2022. Proves the new
    /// available balance matches the commitment the range proof covers.
    pub equality_proof: UncheckedAccount<'info>,

    /// CHECK: context state account, verified by Token-2022. Proves the
    /// remaining available balance is non-negative.
    pub range_proof: UncheckedAccount<'info>,

    pub owner: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
}

impl<'info> WithdrawConfidential<'info> {
    pub fn withdraw_confidential(
        &self,
        amount: u64,
        new_decryptable_available_balance: [u8; 36],
    ) -> Result<()> {
        let decimals = mint_decimals(&self.mint)?;

        let ix = inner_withdraw(
            &self.token_program.key(),
            &self.token_account.key(),
            &self.mint.key(),
            amount,
            decimals,
            &DecryptableBalance::from(new_decryptable_available_balance),
            &self.owner.key(),
            &[],
            ProofLocation::ContextStateAccount(&self.equality_proof.key()),
            ProofLocation::ContextStateAccount(&self.range_proof.key()),
        )?;
        invoke(
            &ix,
            &[
                self.token_account.to_account_info(),
                self.mint.to_account_info(),
                self.equality_proof.to_account_info(),
                self.range_proof.to_account_info(),
                self.owner.to_account_info(),
                self.token_program.to_account_info(),
            ],
        )?;

        msg!("withdrew {} from the confidential balance", amount);
        Ok(())
    }
}
