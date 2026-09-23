use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    default_account_state_initialize, initialize_mint2, metadata_pointer_initialize,
    mint_close_authority_initialize,
    spl_token_2022::{
        extension::ExtensionType,
        state::{AccountState, Mint as MintState},
    },
    spl_token_metadata_interface::state::TokenMetadata,
    token_metadata_initialize, transfer_fee_initialize, DefaultAccountStateInitialize,
    InitializeMint2, MetadataPointerInitialize, MintCloseAuthorityInitialize, Token2022,
    TokenMetadataInitialize, TransferFeeInitialize,
};

pub struct MintBuilder<'a, 'info> {
    pub issuer: &'a Signer<'info>,
    pub mint: &'a Signer<'info>,
    pub token_program: &'a Program<'info, Token2022>,
    pub system_program: &'a Program<'info, System>,
}

impl<'info> MintBuilder<'_, 'info> {
    pub fn create_account(
        &self,
        extensions: &[ExtensionType],
        metadata: &TokenMetadata,
    ) -> Result<()> {
        let space = ExtensionType::try_calculate_account_len::<MintState>(extensions)?;
        let lamports = Rent::get()?.minimum_balance(space + metadata.tlv_size_of()?);

        anchor_lang::system_program::create_account(
            CpiContext::new(
                self.system_program.key(),
                anchor_lang::system_program::CreateAccount {
                    from: self.issuer.to_account_info(),
                    to: self.mint.to_account_info(),
                },
            ),
            lamports,
            space as u64,
            &self.token_program.key(),
        )
    }

    pub fn initialize_stablecoin_extensions(
        &self,
        transfer_fee_basis_points: u16,
        maximum_fee: u64,
    ) -> Result<()> {
        let issuer = self.issuer.key();
        let token_program = self.token_program.key();
        let token_program_info = self.token_program.to_account_info();
        let mint_info = self.mint.to_account_info();

        transfer_fee_initialize(
            CpiContext::new(
                token_program,
                TransferFeeInitialize {
                    token_program_id: token_program_info.clone(),
                    mint: mint_info.clone(),
                },
            ),
            Some(&issuer),
            Some(&issuer),
            transfer_fee_basis_points,
            maximum_fee,
        )?;

        metadata_pointer_initialize(
            CpiContext::new(
                token_program,
                MetadataPointerInitialize {
                    token_program_id: token_program_info.clone(),
                    mint: mint_info.clone(),
                },
            ),
            Some(issuer),
            Some(self.mint.key()),
        )?;

        default_account_state_initialize(
            CpiContext::new(
                token_program,
                DefaultAccountStateInitialize {
                    token_program_id: token_program_info.clone(),
                    mint: mint_info.clone(),
                },
            ),
            &AccountState::Frozen,
        )?;

        mint_close_authority_initialize(
            CpiContext::new(
                token_program,
                MintCloseAuthorityInitialize {
                    token_program_id: token_program_info,
                    mint: mint_info,
                },
            ),
            Some(&issuer),
        )
    }

    pub fn initialize_mint(&self, decimals: u8) -> Result<()> {
        let issuer = self.issuer.key();

        initialize_mint2(
            CpiContext::new(
                self.token_program.key(),
                InitializeMint2 {
                    mint: self.mint.to_account_info(),
                },
            ),
            decimals,
            &issuer,
            Some(&issuer),
        )
    }

    pub fn initialize_metadata(&self, metadata: TokenMetadata) -> Result<()> {
        token_metadata_initialize(
            CpiContext::new(
                self.token_program.key(),
                TokenMetadataInitialize {
                    program_id: self.token_program.to_account_info(),
                    metadata: self.mint.to_account_info(),
                    update_authority: self.issuer.to_account_info(),
                    mint_authority: self.issuer.to_account_info(),
                    mint: self.mint.to_account_info(),
                },
            ),
            metadata.name,
            metadata.symbol,
            metadata.uri,
        )
    }
}

pub fn stablecoin_metadata(name: String, symbol: String, uri: String) -> TokenMetadata {
    TokenMetadata {
        name,
        symbol,
        uri,
        ..Default::default()
    }
}
