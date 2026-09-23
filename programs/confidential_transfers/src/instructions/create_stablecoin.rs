use anchor_lang::prelude::*;
use anchor_spl::token_interface::Token2022;

use crate::{
    constants::STABLECOIN_MINT_EXTENSIONS,
    mint_builder::{stablecoin_metadata, MintBuilder},
};

#[derive(Accounts)]
pub struct CreateStablecoin<'info> {
    #[account(mut)]
    pub issuer: Signer<'info>,

    #[account(mut)]
    pub mint: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

impl<'info> CreateStablecoin<'info> {
    pub fn create_stablecoin(
        &self,
        decimals: u8,
        transfer_fee_basis_points: u16,
        maximum_fee: u64,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        let builder = MintBuilder {
            issuer: &self.issuer,
            mint: &self.mint,
            token_program: &self.token_program,
            system_program: &self.system_program,
        };
        let metadata = stablecoin_metadata(name, symbol, uri);

        builder.create_account(&STABLECOIN_MINT_EXTENSIONS, &metadata)?;
        builder.initialize_stablecoin_extensions(transfer_fee_basis_points, maximum_fee)?;
        builder.initialize_mint(decimals)?;
        builder.initialize_metadata(metadata)?;

        msg!("stablecoin mint {} created", self.mint.key());
        Ok(())
    }
}
