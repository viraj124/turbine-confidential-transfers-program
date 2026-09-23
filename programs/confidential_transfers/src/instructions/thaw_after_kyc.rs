use anchor_lang::prelude::*;
use anchor_spl::token_interface::{thaw_account, ThawAccount, Token2022};

#[derive(Accounts)]
pub struct ThawAfterKyc<'info> {
    /// CHECK: validated by Token-2022, which rejects it unless it belongs to
    /// `mint` and is currently frozen.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022 during the thaw.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    pub freeze_authority: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
}

impl<'info> ThawAfterKyc<'info> {
    pub fn thaw_after_kyc(&self) -> Result<()> {
        thaw_account(CpiContext::new(
            self.token_program.key(),
            ThawAccount {
                account: self.token_account.to_account_info(),
                mint: self.mint.to_account_info(),
                authority: self.freeze_authority.to_account_info(),
            },
        ))?;

        msg!(
            "token account {} thawed after KYC",
            self.token_account.key()
        );
        Ok(())
    }
}
