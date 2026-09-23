use anchor_lang::{prelude::*, solana_program::program::invoke};
use anchor_spl::token_interface::{
    spl_token_2022::extension::confidential_transfer::instruction::approve_account, Token2022,
};

#[derive(Accounts)]
pub struct ApproveConfidentialAccount<'info> {
    /// CHECK: validated by Token-2022, which requires it to be configured for
    /// confidential transfers and to belong to `mint`.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    pub authority: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
}

impl<'info> ApproveConfidentialAccount<'info> {
    pub fn approve_confidential_account(&self) -> Result<()> {
        let ix = approve_account(
            &self.token_program.key(),
            &self.token_account.key(),
            &self.mint.key(),
            &self.authority.key(),
            &[],
        )?;
        invoke(
            &ix,
            &[
                self.token_account.to_account_info(),
                self.mint.to_account_info(),
                self.authority.to_account_info(),
                self.token_program.to_account_info(),
            ],
        )?;

        msg!(
            "token account {} approved for confidential transfers",
            self.token_account.key()
        );
        Ok(())
    }
}
