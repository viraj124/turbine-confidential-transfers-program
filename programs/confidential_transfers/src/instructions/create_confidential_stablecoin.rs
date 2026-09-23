use anchor_lang::{prelude::*, solana_program::program::invoke};
use anchor_spl::token_interface::{
    permanent_delegate_initialize,
    spl_token_2022::{
        extension::{
            confidential_transfer::instruction as confidential_transfer_instruction,
            confidential_transfer_fee::instruction as confidential_transfer_fee_instruction,
        },
        solana_zk_sdk::encryption::pod::elgamal::PodElGamalPubkey,
    },
    PermanentDelegateInitialize, Token2022,
};

use crate::{
    constants::CONFIDENTIAL_STABLECOIN_MINT_EXTENSIONS,
    mint_builder::{stablecoin_metadata, MintBuilder},
};

#[derive(Accounts)]
pub struct CreateConfidentialStablecoin<'info> {
    #[account(mut)]
    pub issuer: Signer<'info>,

    #[account(mut)]
    pub mint: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

impl<'info> CreateConfidentialStablecoin<'info> {
    #[allow(clippy::too_many_arguments)]
    pub fn create_confidential_stablecoin(
        &self,
        decimals: u8,
        transfer_fee_basis_points: u16,
        maximum_fee: u64,
        name: String,
        symbol: String,
        uri: String,
        withdraw_withheld_authority_elgamal_pubkey: [u8; 32],
        auditor_elgamal_pubkey: Option<[u8; 32]>,
    ) -> Result<()> {
        let builder = MintBuilder {
            issuer: &self.issuer,
            mint: &self.mint,
            token_program: &self.token_program,
            system_program: &self.system_program,
        };
        let metadata = stablecoin_metadata(name, symbol, uri);

        builder.create_account(&CONFIDENTIAL_STABLECOIN_MINT_EXTENSIONS, &metadata)?;
        builder.initialize_stablecoin_extensions(transfer_fee_basis_points, maximum_fee)?;
        self.initialize_seizure_authority()?;
        self.initialize_confidential_transfers(
            withdraw_withheld_authority_elgamal_pubkey,
            auditor_elgamal_pubkey,
        )?;
        builder.initialize_mint(decimals)?;
        builder.initialize_metadata(metadata)?;

        msg!("confidential stablecoin mint {} created", self.mint.key());
        Ok(())
    }

    fn initialize_seizure_authority(&self) -> Result<()> {
        permanent_delegate_initialize(
            CpiContext::new(
                self.token_program.key(),
                PermanentDelegateInitialize {
                    token_program_id: self.token_program.to_account_info(),
                    mint: self.mint.to_account_info(),
                },
            ),
            &self.issuer.key(),
        )
    }

    fn initialize_confidential_transfers(
        &self,
        withdraw_withheld_authority_elgamal_pubkey: [u8; 32],
        auditor_elgamal_pubkey: Option<[u8; 32]>,
    ) -> Result<()> {
        let token_program = self.token_program.key();
        let mint = self.mint.key();
        let issuer = self.issuer.key();
        let accounts = [
            self.mint.to_account_info(),
            self.token_program.to_account_info(),
        ];

        invoke(
            &confidential_transfer_instruction::initialize_mint(
                &token_program,
                &mint,
                Some(issuer),
                false,
                auditor_elgamal_pubkey.map(PodElGamalPubkey::from),
            )?,
            &accounts,
        )?;

        invoke(
            &confidential_transfer_fee_instruction::initialize_confidential_transfer_fee_config(
                &token_program,
                &mint,
                Some(issuer),
                &PodElGamalPubkey::from(withdraw_withheld_authority_elgamal_pubkey),
            )?,
            &accounts,
        )?;

        Ok(())
    }
}
