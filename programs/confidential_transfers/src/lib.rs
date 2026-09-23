#![allow(clippy::diverging_sub_expression)]

pub mod constants;
pub mod error;
pub mod instructions;
pub mod mint_builder;
pub mod token_state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;

declare_id!("Ejfss9ECV2SVzSej28DDkMybZd9sEcgjQXqgXHdx9KK");

#[program]
pub mod confidential_transfers {
    use super::*;

    pub fn create_stablecoin(
        ctx: Context<CreateStablecoin>,
        decimals: u8,
        transfer_fee_basis_points: u16,
        maximum_fee: u64,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        ctx.accounts.create_stablecoin(
            decimals,
            transfer_fee_basis_points,
            maximum_fee,
            name,
            symbol,
            uri,
        )
    }

    pub fn transfer_with_fee(ctx: Context<TransferWithFee>, amount: u64) -> Result<()> {
        ctx.accounts.transfer_with_fee(amount)
    }

    pub fn thaw_after_kyc(ctx: Context<ThawAfterKyc>) -> Result<()> {
        ctx.accounts.thaw_after_kyc()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_confidential_stablecoin(
        ctx: Context<CreateConfidentialStablecoin>,
        decimals: u8,
        transfer_fee_basis_points: u16,
        maximum_fee: u64,
        name: String,
        symbol: String,
        uri: String,
        withdraw_withheld_authority_elgamal_pubkey: [u8; 32],
        auditor_elgamal_pubkey: Option<[u8; 32]>,
    ) -> Result<()> {
        ctx.accounts.create_confidential_stablecoin(
            decimals,
            transfer_fee_basis_points,
            maximum_fee,
            name,
            symbol,
            uri,
            withdraw_withheld_authority_elgamal_pubkey,
            auditor_elgamal_pubkey,
        )
    }

    pub fn seize(ctx: Context<Seize>, amount: u64) -> Result<()> {
        ctx.accounts.seize(amount)
    }

    pub fn configure_confidential_account(
        ctx: Context<ConfigureConfidentialAccount>,
        decryptable_zero_balance: [u8; 36],
    ) -> Result<()> {
        ctx.accounts
            .configure_confidential_account(decryptable_zero_balance)
    }

    pub fn approve_confidential_account(ctx: Context<ApproveConfidentialAccount>) -> Result<()> {
        ctx.accounts.approve_confidential_account()
    }

    pub fn deposit_confidential(ctx: Context<DepositConfidential>, amount: u64) -> Result<()> {
        ctx.accounts.deposit_confidential(amount)
    }

    pub fn apply_pending_balance(
        ctx: Context<ApplyPendingBalance>,
        expected_pending_balance_credit_counter: u64,
        new_decryptable_available_balance: [u8; 36],
    ) -> Result<()> {
        ctx.accounts.apply_pending_balance(
            expected_pending_balance_credit_counter,
            new_decryptable_available_balance,
        )
    }

    pub fn confidential_transfer(
        ctx: Context<ConfidentialTransfer>,
        new_source_decryptable_available_balance: [u8; 36],
        transfer_amount_auditor_ciphertext_lo: [u8; 64],
        transfer_amount_auditor_ciphertext_hi: [u8; 64],
    ) -> Result<()> {
        ctx.accounts.confidential_transfer(
            new_source_decryptable_available_balance,
            transfer_amount_auditor_ciphertext_lo,
            transfer_amount_auditor_ciphertext_hi,
        )
    }

    pub fn withdraw_confidential(
        ctx: Context<WithdrawConfidential>,
        amount: u64,
        new_decryptable_available_balance: [u8; 36],
    ) -> Result<()> {
        ctx.accounts
            .withdraw_confidential(amount, new_decryptable_available_balance)
    }
}
