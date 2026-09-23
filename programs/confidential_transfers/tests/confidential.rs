mod common;

use common::*;
use solana_signer::Signer;
use t22new::extension::{
    confidential_transfer::ConfidentialTransferMint,
    confidential_transfer_fee::ConfidentialTransferFeeConfig,
    permanent_delegate::PermanentDelegate, BaseStateWithExtensions, ExtensionType,
};
use zkpod::encryption::elgamal::PodElGamalPubkey;

mod reissued_mint {
    use super::*;

    #[test]
    fn carries_the_original_four_extensions_plus_three() {
        let coin = TestCoin::new();

        let extensions = coin.with_mint(|mint| mint.get_extension_types().unwrap());
        for extension in [
            ExtensionType::TransferFeeConfig,
            ExtensionType::MetadataPointer,
            ExtensionType::DefaultAccountState,
            ExtensionType::MintCloseAuthority,
            ExtensionType::PermanentDelegate,
            ExtensionType::ConfidentialTransferMint,
            ExtensionType::ConfidentialTransferFeeConfig,
            ExtensionType::TokenMetadata,
        ] {
            assert!(extensions.contains(&extension), "missing {extension:?}");
        }
    }

    #[test]
    fn makes_the_issuer_the_seizure_authority() {
        let coin = TestCoin::new();

        let delegate = coin.with_mint(|mint| {
            Option::from(mint.get_extension::<PermanentDelegate>().unwrap().delegate)
        });
        assert_eq!(delegate, Some(coin.issuer.pubkey()));
    }

    #[test]
    fn approves_confidential_accounts_manually() {
        let coin = TestCoin::new();

        let (auto_approve, authority, auditor) = coin.with_mint(|mint| {
            let config = mint.get_extension::<ConfidentialTransferMint>().unwrap();
            (
                bool::from(config.auto_approve_new_accounts),
                Option::from(config.authority),
                Option::from(config.auditor_elgamal_pubkey),
            )
        });

        assert!(!auto_approve, "approve policy must be manual");
        assert_eq!(authority, Some(coin.issuer.pubkey()));
        assert_eq!(
            auditor,
            Some(PodElGamalPubkey::from(*coin.auditor.pubkey()))
        );
    }

    #[test]
    fn withholds_confidential_fees_for_the_issuer() {
        let coin = TestCoin::new();

        let withheld_key = coin.with_mint(|mint| {
            mint.get_extension::<ConfidentialTransferFeeConfig>()
                .unwrap()
                .withdraw_withheld_authority_elgamal_pubkey
        });
        assert_eq!(
            withheld_key,
            PodElGamalPubkey::from(*coin.withdraw_withheld_authority.pubkey())
        );
    }

    #[test]
    fn keeps_new_accounts_frozen_until_kyc() {
        let mut coin = TestCoin::new();
        let holder = coin.new_holder();

        assert!(coin.is_frozen(&holder.account));
    }
}

mod configure {
    use super::*;

    #[test]
    fn anyone_can_create_the_account_but_only_the_owner_can_configure_it() {
        let mut coin = TestCoin::new();
        let holder = coin.new_holder();
        let stranger = solana_keypair::Keypair::new();
        coin.svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();

        assert_fails(
            coin.configure_as(&holder, &stranger),
            "owner does not match",
        );
        assert!(!coin.has_confidential_extension(&holder.account));

        coin.configure(&holder).unwrap();
        assert!(coin.has_confidential_extension(&holder.account));
    }

    #[test]
    fn registers_the_owners_elgamal_key() {
        let mut coin = TestCoin::new();
        let holder = coin.new_holder();

        coin.configure(&holder).unwrap();

        let registered = coin.confidential_account(&holder.account).elgamal_pubkey;
        assert_eq!(registered, PodElGamalPubkey::from(*holder.elgamal.pubkey()));
    }

    #[test]
    fn leaves_the_account_unapproved_under_the_manual_policy() {
        let mut coin = TestCoin::new();
        let holder = coin.new_holder();

        coin.configure(&holder).unwrap();

        assert!(!bool::from(
            coin.confidential_account(&holder.account).approved
        ));
    }
}

mod approve {
    use super::*;

    #[test]
    fn an_unapproved_account_cannot_deposit() {
        let mut coin = TestCoin::new();
        let holder = coin.new_holder();
        coin.thaw(&holder.account).unwrap();
        coin.mint_to(&holder.account, 10 * ONE_TOKEN).unwrap();
        coin.configure(&holder).unwrap();

        assert_fails(
            coin.deposit(&holder, ONE_TOKEN),
            "not approved for confidential transfers",
        );

        coin.approve(&holder.account).unwrap();
        coin.deposit(&holder, ONE_TOKEN).unwrap();
    }

    #[test]
    fn only_the_confidential_transfer_authority_can_approve() {
        let mut coin = TestCoin::new();
        let holder = coin.new_holder();
        coin.configure(&holder).unwrap();

        assert_fails(
            coin.approve_as(&holder.account, &holder.owner.insecure_clone()),
            "missing required signature",
        );
        assert!(!bool::from(
            coin.confidential_account(&holder.account).approved
        ));
    }

    #[test]
    fn kyc_still_gates_confidential_funds() {
        let mut coin = TestCoin::new();
        let holder = coin.new_holder();
        coin.configure(&holder).unwrap();
        coin.approve(&holder.account).unwrap();

        assert_fails(coin.deposit(&holder, ONE_TOKEN), "account is frozen");
    }
}

mod lifecycle {
    use super::*;

    #[test]
    fn deposit_apply_transfer_apply_withdraw() {
        let mut coin = TestCoin::new();
        let alice = coin.confidential_holder(100 * ONE_TOKEN);
        let bob = coin.confidential_holder(0);

        coin.deposit(&alice, 100 * ONE_TOKEN).unwrap();
        assert_eq!(coin.public_balance(&alice.account), 0);
        assert_eq!(coin.pending(&alice), 100 * ONE_TOKEN);
        assert_eq!(coin.available(&alice), 0);

        coin.apply_pending(&alice).unwrap();
        assert_eq!(coin.pending(&alice), 0);
        assert_eq!(coin.available(&alice), 100 * ONE_TOKEN);
        assert_eq!(coin.decryptable_available(&alice), 100 * ONE_TOKEN);

        let amount = 40 * ONE_TOKEN;
        let fee = expected_fee(amount);
        coin.confidential_transfer(&alice, &bob, amount).unwrap();
        assert_eq!(coin.available(&alice), 60 * ONE_TOKEN);
        assert_eq!(coin.decryptable_available(&alice), 60 * ONE_TOKEN);
        assert_eq!(coin.pending(&bob), amount - fee);
        assert_eq!(coin.available(&bob), 0);

        coin.apply_pending(&bob).unwrap();
        assert_eq!(coin.available(&bob), amount - fee);

        coin.withdraw(&bob, amount - fee).unwrap();
        assert_eq!(coin.available(&bob), 0);
        assert_eq!(coin.public_balance(&bob.account), amount - fee);
    }

    #[test]
    fn withdrawal_needs_the_pending_balance_applied_first() {
        let mut coin = TestCoin::new();
        let alice = coin.confidential_holder(10 * ONE_TOKEN);
        coin.deposit(&alice, 10 * ONE_TOKEN).unwrap();

        assert_fails(coin.withdraw(&alice, 10 * ONE_TOKEN), "NotEnoughFunds");
        assert_eq!(coin.public_balance(&alice.account), 0);

        coin.apply_pending(&alice).unwrap();
        coin.withdraw(&alice, 10 * ONE_TOKEN).unwrap();
        assert_eq!(coin.public_balance(&alice.account), 10 * ONE_TOKEN);
    }

    #[test]
    fn received_funds_are_not_spendable_until_the_recipient_applies() {
        let mut coin = TestCoin::new();
        let alice = coin.confidential_holder(10 * ONE_TOKEN);
        let bob = coin.confidential_holder(0);
        coin.deposit(&alice, 10 * ONE_TOKEN).unwrap();
        coin.apply_pending(&alice).unwrap();

        coin.confidential_transfer(&alice, &bob, 5 * ONE_TOKEN)
            .unwrap();

        assert_fails(coin.withdraw(&bob, ONE_TOKEN), "NotEnoughFunds");
        coin.apply_pending(&bob).unwrap();
        coin.withdraw(&bob, ONE_TOKEN).unwrap();
    }

    #[test]
    fn a_transfer_cannot_exceed_the_available_balance() {
        let mut coin = TestCoin::new();
        let alice = coin.confidential_holder(10 * ONE_TOKEN);
        let bob = coin.confidential_holder(0);
        coin.deposit(&alice, 10 * ONE_TOKEN).unwrap();
        coin.apply_pending(&alice).unwrap();

        let result = coin
            .confidential_transfer(&alice, &bob, 11 * ONE_TOKEN)
            .map(|_| ());

        assert_fails(result, "proof generation failed");
        assert_eq!(coin.available(&alice), 10 * ONE_TOKEN);
    }

    #[test]
    fn the_auditor_sees_the_amount_and_the_issuer_can_read_the_fee() {
        let mut coin = TestCoin::new();
        let alice = coin.confidential_holder(100 * ONE_TOKEN);
        let bob = coin.confidential_holder(0);
        coin.deposit(&alice, 100 * ONE_TOKEN).unwrap();
        coin.apply_pending(&alice).unwrap();

        let amount = 25 * ONE_TOKEN;
        let audited = coin.confidential_transfer(&alice, &bob, amount).unwrap();

        assert_eq!(coin.audit(&audited), amount);
        assert_eq!(coin.withheld_fee(&bob.account), expected_fee(amount));
    }

    #[test]
    fn the_fee_is_capped_on_large_confidential_transfers() {
        let mut coin = TestCoin::new();
        let alice = coin.confidential_holder(1_000 * ONE_TOKEN);
        let bob = coin.confidential_holder(0);
        coin.deposit(&alice, 1_000 * ONE_TOKEN).unwrap();
        coin.apply_pending(&alice).unwrap();

        let amount = 1_000 * ONE_TOKEN;
        coin.confidential_transfer(&alice, &bob, amount).unwrap();

        assert_eq!(coin.withheld_fee(&bob.account), MAXIMUM_FEE);
        assert_eq!(coin.pending(&bob), amount - MAXIMUM_FEE);
    }
}

mod seizure_gap {
    use super::*;

    #[test]
    fn the_permanent_delegate_seizes_public_funds_without_consent() {
        let mut coin = TestCoin::new();
        let sanctioned = coin.confidential_holder(50 * ONE_TOKEN);
        let treasury = coin.confidential_holder(0);

        let amount = 50 * ONE_TOKEN;
        coin.seize(&sanctioned.account, &treasury.account, amount)
            .unwrap();

        assert_eq!(coin.public_balance(&sanctioned.account), 0);
        assert_eq!(
            coin.public_balance(&treasury.account),
            amount - expected_fee(amount)
        );
    }

    #[test]
    fn funds_moved_into_the_confidential_balance_are_out_of_reach() {
        let mut coin = TestCoin::new();
        let sanctioned = coin.confidential_holder(100 * ONE_TOKEN);
        let treasury = coin.confidential_holder(0);

        coin.deposit(&sanctioned, 70 * ONE_TOKEN).unwrap();
        coin.apply_pending(&sanctioned).unwrap();

        coin.seize(&sanctioned.account, &treasury.account, 30 * ONE_TOKEN)
            .unwrap();
        assert_fails(
            coin.seize(&sanctioned.account, &treasury.account, ONE_TOKEN),
            "insufficient funds",
        );

        assert_eq!(coin.public_balance(&sanctioned.account), 0);
        assert_eq!(coin.available(&sanctioned), 70 * ONE_TOKEN);
    }

    #[test]
    fn pending_confidential_funds_are_out_of_reach_too() {
        let mut coin = TestCoin::new();
        let sanctioned = coin.confidential_holder(20 * ONE_TOKEN);
        let treasury = coin.confidential_holder(0);

        coin.deposit(&sanctioned, 20 * ONE_TOKEN).unwrap();

        assert_fails(
            coin.seize(&sanctioned.account, &treasury.account, ONE_TOKEN),
            "insufficient funds",
        );
        assert_eq!(coin.pending(&sanctioned), 20 * ONE_TOKEN);
    }

    #[test]
    fn freezing_stops_the_holder_but_recovers_nothing() {
        let mut coin = TestCoin::new();
        let sanctioned = coin.confidential_holder(100 * ONE_TOKEN);
        let accomplice = coin.confidential_holder(0);
        let treasury = coin.confidential_holder(0);
        coin.deposit(&sanctioned, 100 * ONE_TOKEN).unwrap();
        coin.apply_pending(&sanctioned).unwrap();

        coin.freeze(&sanctioned.account).unwrap();

        assert_fails(coin.withdraw(&sanctioned, ONE_TOKEN), "account is frozen");
        let moved = coin
            .confidential_transfer(&sanctioned, &accomplice, ONE_TOKEN)
            .map(|_| ());
        assert_fails(moved, "account is frozen");

        assert_fails(
            coin.seize(&sanctioned.account, &treasury.account, ONE_TOKEN),
            "account is frozen",
        );
        assert_eq!(coin.available(&sanctioned), 100 * ONE_TOKEN);
    }
}
