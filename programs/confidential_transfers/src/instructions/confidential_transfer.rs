use anchor_lang::{prelude::*, solana_program::program::invoke};
use anchor_spl::token_interface::{
    spl_token_2022::{
        extension::confidential_transfer::{
            instruction::inner_transfer_with_fee, DecryptableBalance,
        },
        solana_zk_sdk::encryption::pod::elgamal::PodElGamalCiphertext,
    },
    Token2022,
};
use spl_token_confidential_transfer_proof_extraction::instruction::ProofLocation;

#[derive(Accounts)]
pub struct ConfidentialTransfer<'info> {
    /// CHECK: validated by Token-2022, which requires it to be configured,
    /// approved and not frozen.
    #[account(mut, owner = token_program.key())]
    pub source: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022, which reads the fee rate and the
    /// auditor and withheld-fee keys from it.
    #[account(owner = token_program.key())]
    pub mint: UncheckedAccount<'info>,

    /// CHECK: validated by Token-2022, which requires it to be configured,
    /// approved and not frozen.
    #[account(mut, owner = token_program.key())]
    pub destination: UncheckedAccount<'info>,

    /// CHECK: context state account, verified by Token-2022. Proves the new
    /// available balance matches the commitment the range proof covers.
    pub equality_proof: UncheckedAccount<'info>,

    /// CHECK: context state account, verified by Token-2022. Proves the
    /// amount is encrypted correctly for sender, recipient and auditor.
    pub transfer_amount_ciphertext_validity_proof: UncheckedAccount<'info>,

    /// CHECK: context state account, verified by Token-2022. Proves the
    /// hidden fee is the mint's rate applied to the hidden amount, capped.
    pub fee_sigma_proof: UncheckedAccount<'info>,

    /// CHECK: context state account, verified by Token-2022. Proves the fee
    /// is encrypted correctly for the recipient and the withheld authority.
    pub fee_ciphertext_validity_proof: UncheckedAccount<'info>,

    /// CHECK: context state account, verified by Token-2022. Proves the
    /// amount, fee and remaining balance are all non-negative.
    pub range_proof: UncheckedAccount<'info>,

    pub owner: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
}

impl<'info> ConfidentialTransfer<'info> {
    pub fn confidential_transfer(
        &self,
        new_source_decryptable_available_balance: [u8; 36],
        transfer_amount_auditor_ciphertext_lo: [u8; 64],
        transfer_amount_auditor_ciphertext_hi: [u8; 64],
    ) -> Result<()> {
        let ix = inner_transfer_with_fee(
            &self.token_program.key(),
            &self.source.key(),
            &self.mint.key(),
            &self.destination.key(),
            &DecryptableBalance::from(new_source_decryptable_available_balance),
            &PodElGamalCiphertext::from(transfer_amount_auditor_ciphertext_lo),
            &PodElGamalCiphertext::from(transfer_amount_auditor_ciphertext_hi),
            &self.owner.key(),
            &[],
            ProofLocation::ContextStateAccount(&self.equality_proof.key()),
            ProofLocation::ContextStateAccount(
                &self.transfer_amount_ciphertext_validity_proof.key(),
            ),
            ProofLocation::ContextStateAccount(&self.fee_sigma_proof.key()),
            ProofLocation::ContextStateAccount(&self.fee_ciphertext_validity_proof.key()),
            ProofLocation::ContextStateAccount(&self.range_proof.key()),
        )?;
        invoke(
            &ix,
            &[
                self.source.to_account_info(),
                self.mint.to_account_info(),
                self.destination.to_account_info(),
                self.equality_proof.to_account_info(),
                self.transfer_amount_ciphertext_validity_proof
                    .to_account_info(),
                self.fee_sigma_proof.to_account_info(),
                self.fee_ciphertext_validity_proof.to_account_info(),
                self.range_proof.to_account_info(),
                self.owner.to_account_info(),
                self.token_program.to_account_info(),
            ],
        )?;

        msg!("confidential transfer to {}", self.destination.key());
        Ok(())
    }
}
