use anchor_lang::{prelude::*, solana_program::program::invoke};
use anchor_spl::token_interface::{
    spl_token_2022::extension::confidential_transfer::{
        instruction::inner_apply_pending_balance, DecryptableBalance,
    },
    Token2022,
};

#[derive(Accounts)]
pub struct ApplyPendingBalance<'info> {
    /// CHECK: validated by Token-2022, which requires it to be configured for
    /// confidential transfers.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    pub owner: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
}

impl<'info> ApplyPendingBalance<'info> {
    pub fn apply_pending_balance(
        &self,
        expected_pending_balance_credit_counter: u64,
        new_decryptable_available_balance: [u8; 36],
    ) -> Result<()> {
        let ix = inner_apply_pending_balance(
            &self.token_program.key(),
            &self.token_account.key(),
            expected_pending_balance_credit_counter,
            &DecryptableBalance::from(new_decryptable_available_balance),
            &self.owner.key(),
            &[],
        )?;
        invoke(
            &ix,
            &[
                self.token_account.to_account_info(),
                self.owner.to_account_info(),
                self.token_program.to_account_info(),
            ],
        )?;

        msg!("pending balance applied");
        Ok(())
    }
}
