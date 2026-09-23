use anchor_lang::{prelude::*, solana_program::program::invoke};
use anchor_spl::token_interface::{
    reallocate,
    spl_token_2022::extension::confidential_transfer::{
        instruction::inner_configure_account, DecryptableBalance,
    },
    Reallocate, Token2022,
};
use spl_token_confidential_transfer_proof_extraction::instruction::ProofLocation;

use crate::constants::{CONFIDENTIAL_ACCOUNT_EXTENSIONS, MAXIMUM_PENDING_BALANCE_CREDIT_COUNTER};

#[derive(Accounts)]
pub struct ConfigureConfidentialAccount<'info> {
    /// CHECK: validated by Token-2022, which rejects it unless `owner` owns it
    /// and it belongs to `mint`.
    #[account(mut, owner = token_program.key())]
    pub token_account: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022, which requires the confidential
    /// transfer extension on it.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    /// CHECK: Token-2022 checks it is owned by the ZK ElGamal proof program
    /// and holds a proof of the right type.
    pub pubkey_validity_proof: UncheckedAccount<'info>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

impl<'info> ConfigureConfidentialAccount<'info> {
    pub fn configure_confidential_account(&self, decryptable_zero_balance: [u8; 36]) -> Result<()> {
        self.enable_confidential_extensions()?;

        let ix = inner_configure_account(
            &self.token_program.key(),
            &self.token_account.key(),
            &self.mint.key(),
            &DecryptableBalance::from(decryptable_zero_balance),
            MAXIMUM_PENDING_BALANCE_CREDIT_COUNTER,
            &self.owner.key(),
            &[],
            ProofLocation::ContextStateAccount(&self.pubkey_validity_proof.key()),
        )?;
        invoke(
            &ix,
            &[
                self.token_account.to_account_info(),
                self.mint.to_account_info(),
                self.pubkey_validity_proof.to_account_info(),
                self.owner.to_account_info(),
                self.token_program.to_account_info(),
            ],
        )?;

        msg!("token account {} configured", self.token_account.key());
        Ok(())
    }

    fn enable_confidential_extensions(&self) -> Result<()> {
        reallocate(
            CpiContext::new(
                self.token_program.key(),
                Reallocate {
                    account: self.token_account.to_account_info(),
                    payer: self.owner.to_account_info(),
                    system_program: self.system_program.to_account_info(),
                    authority: self.owner.to_account_info(),
                },
            ),
            &CONFIDENTIAL_ACCOUNT_EXTENSIONS,
        )
    }
}
