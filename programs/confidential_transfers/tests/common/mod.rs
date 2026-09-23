use anchor_lang::{
    prelude::Pubkey,
    solana_program::instruction::{AccountMeta, Instruction},
    system_program, InstructionData, ToAccountMetas,
};
use bytemuck::Pod;
use confidential_transfers::{accounts, instruction, ID as PROGRAM_ID};
use litesvm::LiteSVM;
use proofgen::{
    transfer_with_fee::transfer_with_fee_split_proof_data, try_combine_lo_hi_ciphertexts,
    withdraw::withdraw_proof_data, TRANSFER_AMOUNT_LO_BITS,
};
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_keypair::Keypair;
use solana_signer::Signer;
use solana_transaction::Transaction;
use t22new::{
    extension::{
        confidential_transfer::{ConfidentialTransferAccount, PENDING_BALANCE_LO_BIT_LENGTH},
        confidential_transfer_fee::ConfidentialTransferFeeAmount,
        BaseStateWithExtensions, StateWithExtensions,
    },
    state::{Account, Mint},
};
use zk::{
    encryption::{
        auth_encryption::{AeCiphertext, AeKey},
        elgamal::{ElGamalCiphertext, ElGamalKeypair, ElGamalPubkey, ElGamalSecretKey},
    },
    zk_elgamal_proof_program::build_pubkey_validity_proof_data,
};
use zkif::{
    instruction::{close_context_state, ContextStateInfo, ProofInstruction},
    proof_data::ZkProofData,
    state::ProofContextState,
};
use zkpod::encryption::{
    auth_encryption::PodAeCiphertext,
    elgamal::{PodElGamalCiphertext, PodElGamalPubkey},
};

pub const DECIMALS: u8 = 6;
pub const ONE_TOKEN: u64 = 1_000_000;
pub const FEE_BASIS_POINTS: u16 = 100;
pub const MAXIMUM_FEE: u64 = 5 * ONE_TOKEN;

const ASSOCIATED_TOKEN_PROGRAM_ID: Pubkey =
    Pubkey::from_str_const("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

const PROOF_COMPUTE_UNITS: u32 = 1_400_000;

pub type TxResult = Result<(), String>;

pub struct Holder {
    pub owner: Keypair,
    pub account: Pubkey,
    pub elgamal: ElGamalKeypair,
    pub aes: AeKey,
}

pub struct AuditorCiphertexts {
    pub lo: PodElGamalCiphertext,
    pub hi: PodElGamalCiphertext,
}

pub struct TestCoin {
    pub svm: LiteSVM,
    pub payer: Keypair,
    pub issuer: Keypair,
    pub mint: Keypair,
    pub auditor: ElGamalKeypair,
    pub withdraw_withheld_authority: ElGamalKeypair,
}

impl TestCoin {
    pub fn new() -> Self {
        let mut coin = Self::without_mint();
        coin.create_mint().expect("create the confidential mint");
        coin
    }

    pub fn without_mint() -> Self {
        let mut svm = LiteSVM::new();
        let program = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/deploy/confidential_transfers.so"
        );
        svm.add_program_from_file(PROGRAM_ID, program)
            .expect("run `anchor build` first");

        let payer = Keypair::new();
        let issuer = Keypair::new();
        svm.airdrop(&payer.pubkey(), 1_000_000_000_000).unwrap();
        svm.airdrop(&issuer.pubkey(), 1_000_000_000_000).unwrap();

        Self {
            svm,
            payer,
            issuer,
            mint: Keypair::new(),
            auditor: ElGamalKeypair::new_rand(),
            withdraw_withheld_authority: ElGamalKeypair::new_rand(),
        }
    }

    pub fn create_mint(&mut self) -> TxResult {
        let ix = Instruction {
            program_id: PROGRAM_ID,
            accounts: accounts::CreateConfidentialStablecoin {
                issuer: self.issuer.pubkey(),
                mint: self.mint.pubkey(),
                token_program: t22new::id(),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
            data: instruction::CreateConfidentialStablecoin {
                decimals: DECIMALS,
                transfer_fee_basis_points: FEE_BASIS_POINTS,
                maximum_fee: MAXIMUM_FEE,
                name: "Remittance USD".into(),
                symbol: "RUSD".into(),
                uri: "https://example.com/rusd.json".into(),
                withdraw_withheld_authority_elgamal_pubkey: pubkey_bytes(
                    self.withdraw_withheld_authority.pubkey(),
                ),
                auditor_elgamal_pubkey: Some(pubkey_bytes(self.auditor.pubkey())),
            }
            .data(),
        };
        let issuer = self.issuer.insecure_clone();
        let mint = self.mint.insecure_clone();
        self.send(&[ix], &[&issuer, &mint])
    }

    pub fn send(&mut self, ixs: &[Instruction], signers: &[&Keypair]) -> TxResult {
        self.svm.expire_blockhash();

        let mut all: Vec<&Keypair> = vec![&self.payer];
        for signer in signers {
            if !all.iter().any(|s| s.pubkey() == signer.pubkey()) {
                all.push(signer);
            }
        }
        let tx = Transaction::new_signed_with_payer(
            ixs,
            Some(&self.payer.pubkey()),
            &all,
            self.svm.latest_blockhash(),
        );

        self.svm
            .send_transaction(tx)
            .map(|_| ())
            .map_err(|failed| format!("{:?}\n{}", failed.err, failed.meta.logs.join("\n")))
    }

    pub fn new_holder(&mut self) -> Holder {
        let owner = Keypair::new();
        self.svm.airdrop(&owner.pubkey(), 1_000_000_000).unwrap();
        let payer = self.payer.insecure_clone();
        let account = self
            .create_ata(&payer, &owner.pubkey())
            .expect("anyone can create an ATA");

        Holder {
            owner,
            account,
            elgamal: ElGamalKeypair::new_rand(),
            aes: AeKey::new_rand(),
        }
    }

    pub fn confidential_holder(&mut self, public: u64) -> Holder {
        let holder = self.new_holder();
        self.thaw(&holder.account).unwrap();
        if public > 0 {
            self.mint_to(&holder.account, public).unwrap();
        }
        self.configure(&holder).unwrap();
        self.approve(&holder.account).unwrap();
        holder
    }

    pub fn create_ata(&mut self, funder: &Keypair, owner: &Pubkey) -> Result<Pubkey, String> {
        let ata = ata_address(owner, &self.mint.pubkey());
        let ix = Instruction {
            program_id: ASSOCIATED_TOKEN_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(funder.pubkey(), true),
                AccountMeta::new(ata, false),
                AccountMeta::new_readonly(*owner, false),
                AccountMeta::new_readonly(self.mint.pubkey(), false),
                AccountMeta::new_readonly(system_program::ID, false),
                AccountMeta::new_readonly(t22new::id(), false),
            ],
            data: vec![0],
        };
        self.send(&[ix], &[funder]).map(|_| ata)
    }

    pub fn thaw(&mut self, account: &Pubkey) -> TxResult {
        let ix = Instruction {
            program_id: PROGRAM_ID,
            accounts: accounts::ThawAfterKyc {
                token_account: *account,
                mint: self.mint.pubkey(),
                freeze_authority: self.issuer.pubkey(),
                token_program: t22new::id(),
            }
            .to_account_metas(None),
            data: instruction::ThawAfterKyc {}.data(),
        };
        let issuer = self.issuer.insecure_clone();
        self.send(&[ix], &[&issuer])
    }

    pub fn freeze(&mut self, account: &Pubkey) -> TxResult {
        let ix = t22new::instruction::freeze_account(
            &t22new::id(),
            account,
            &self.mint.pubkey(),
            &self.issuer.pubkey(),
            &[],
        )
        .unwrap();
        let issuer = self.issuer.insecure_clone();
        self.send(&[ix], &[&issuer])
    }

    pub fn mint_to(&mut self, account: &Pubkey, amount: u64) -> TxResult {
        let ix = t22new::instruction::mint_to(
            &t22new::id(),
            &self.mint.pubkey(),
            account,
            &self.issuer.pubkey(),
            &[],
            amount,
        )
        .unwrap();
        let issuer = self.issuer.insecure_clone();
        self.send(&[ix], &[&issuer])
    }

    pub fn configure_as(&mut self, holder: &Holder, signer: &Keypair) -> TxResult {
        let proof = build_pubkey_validity_proof_data(&holder.elgamal).unwrap();
        let proof_account = self.stage_proof(ProofInstruction::VerifyPubkeyValidity, &proof);

        let ix = Instruction {
            program_id: PROGRAM_ID,
            accounts: accounts::ConfigureConfidentialAccount {
                token_account: holder.account,
                mint: self.mint.pubkey(),
                pubkey_validity_proof: proof_account,
                owner: signer.pubkey(),
                token_program: t22new::id(),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
            data: instruction::ConfigureConfidentialAccount {
                decryptable_zero_balance: ae_bytes(holder.aes.encrypt(0)),
            }
            .data(),
        };
        let result = self.send(&[ix], &[signer]);
        self.close_proofs(&[proof_account]);
        result
    }

    pub fn configure(&mut self, holder: &Holder) -> TxResult {
        self.configure_as(holder, &holder.owner)
    }

    pub fn approve_as(&mut self, account: &Pubkey, authority: &Keypair) -> TxResult {
        let ix = Instruction {
            program_id: PROGRAM_ID,
            accounts: accounts::ApproveConfidentialAccount {
                token_account: *account,
                mint: self.mint.pubkey(),
                authority: authority.pubkey(),
                token_program: t22new::id(),
            }
            .to_account_metas(None),
            data: instruction::ApproveConfidentialAccount {}.data(),
        };
        self.send(&[ix], &[authority])
    }

    pub fn approve(&mut self, account: &Pubkey) -> TxResult {
        let issuer = self.issuer.insecure_clone();
        self.approve_as(account, &issuer)
    }

    pub fn deposit(&mut self, holder: &Holder, amount: u64) -> TxResult {
        let ix = Instruction {
            program_id: PROGRAM_ID,
            accounts: accounts::DepositConfidential {
                token_account: holder.account,
                mint: self.mint.pubkey(),
                owner: holder.owner.pubkey(),
                token_program: t22new::id(),
            }
            .to_account_metas(None),
            data: instruction::DepositConfidential { amount }.data(),
        };
        self.send(&[ix], &[&holder.owner])
    }

    pub fn apply_pending(&mut self, holder: &Holder) -> TxResult {
        let ct = self.confidential_account(&holder.account);
        let counter = u64::from(ct.pending_balance_credit_counter);
        let new_available = self.decryptable_available(holder) + self.pending(holder);

        let ix = Instruction {
            program_id: PROGRAM_ID,
            accounts: accounts::ApplyPendingBalance {
                token_account: holder.account,
                owner: holder.owner.pubkey(),
                token_program: t22new::id(),
            }
            .to_account_metas(None),
            data: instruction::ApplyPendingBalance {
                expected_pending_balance_credit_counter: counter,
                new_decryptable_available_balance: ae_bytes(holder.aes.encrypt(new_available)),
            }
            .data(),
        };
        self.send(&[ix], &[&holder.owner])
    }

    pub fn confidential_transfer(
        &mut self,
        from: &Holder,
        to: &Holder,
        amount: u64,
    ) -> Result<AuditorCiphertexts, String> {
        let ct = self.confidential_account(&from.account);
        let current_available = ElGamalCiphertext::try_from(ct.available_balance).unwrap();
        let current_decryptable = AeCiphertext::try_from(ct.decryptable_available_balance).unwrap();
        let destination_pubkey = self.elgamal_pubkey_of(&to.account);

        let proof = transfer_with_fee_split_proof_data(
            &current_available,
            &current_decryptable,
            amount,
            &from.elgamal,
            &from.aes,
            &destination_pubkey,
            Some(self.auditor.pubkey()),
            self.withdraw_withheld_authority.pubkey(),
            FEE_BASIS_POINTS,
            MAXIMUM_FEE,
        )
        .map_err(|e| format!("proof generation failed: {e:?}"))?;

        let validity = proof.transfer_amount_ciphertext_validity_proof_data_with_ciphertext;
        let proofs = [
            self.stage_proof(
                ProofInstruction::VerifyCiphertextCommitmentEquality,
                &proof.equality_proof_data,
            ),
            self.stage_proof(
                ProofInstruction::VerifyBatchedGroupedCiphertext3HandlesValidity,
                &validity.proof_data,
            ),
            self.stage_proof(
                ProofInstruction::VerifyPercentageWithCap,
                &proof.percentage_with_cap_proof_data,
            ),
            self.stage_proof(
                ProofInstruction::VerifyBatchedGroupedCiphertext2HandlesValidity,
                &proof.fee_ciphertext_validity_proof_data,
            ),
            self.stage_proof(
                ProofInstruction::VerifyBatchedRangeProofU256,
                &proof.range_proof_data,
            ),
        ];

        let remaining = from.aes.decrypt(&current_decryptable).unwrap() - amount;
        let ix = Instruction {
            program_id: PROGRAM_ID,
            accounts: accounts::ConfidentialTransfer {
                source: from.account,
                mint: self.mint.pubkey(),
                destination: to.account,
                equality_proof: proofs[0],
                transfer_amount_ciphertext_validity_proof: proofs[1],
                fee_sigma_proof: proofs[2],
                fee_ciphertext_validity_proof: proofs[3],
                range_proof: proofs[4],
                owner: from.owner.pubkey(),
                token_program: t22new::id(),
            }
            .to_account_metas(None),
            data: instruction::ConfidentialTransfer {
                new_source_decryptable_available_balance: ae_bytes(from.aes.encrypt(remaining)),
                transfer_amount_auditor_ciphertext_lo: bytemuck::cast(validity.ciphertext_lo),
                transfer_amount_auditor_ciphertext_hi: bytemuck::cast(validity.ciphertext_hi),
            }
            .data(),
        };
        let result = self.send(&[with_compute_budget(), ix], &[&from.owner]);
        self.close_proofs(&proofs);

        result.map(|_| AuditorCiphertexts {
            lo: validity.ciphertext_lo,
            hi: validity.ciphertext_hi,
        })
    }

    pub fn withdraw(&mut self, holder: &Holder, amount: u64) -> TxResult {
        let ct = self.confidential_account(&holder.account);
        let current_available = ElGamalCiphertext::try_from(ct.available_balance).unwrap();
        let current_balance = self.decryptable_available(holder);

        let proof =
            withdraw_proof_data(&current_available, current_balance, amount, &holder.elgamal)
                .map_err(|e| format!("proof generation failed: {e:?}"))?;
        let proofs = [
            self.stage_proof(
                ProofInstruction::VerifyCiphertextCommitmentEquality,
                &proof.equality_proof_data,
            ),
            self.stage_proof(
                ProofInstruction::VerifyBatchedRangeProofU64,
                &proof.range_proof_data,
            ),
        ];

        let ix = Instruction {
            program_id: PROGRAM_ID,
            accounts: accounts::WithdrawConfidential {
                token_account: holder.account,
                mint: self.mint.pubkey(),
                equality_proof: proofs[0],
                range_proof: proofs[1],
                owner: holder.owner.pubkey(),
                token_program: t22new::id(),
            }
            .to_account_metas(None),
            data: instruction::WithdrawConfidential {
                amount,
                new_decryptable_available_balance: ae_bytes(
                    holder.aes.encrypt(current_balance - amount),
                ),
            }
            .data(),
        };
        let result = self.send(&[with_compute_budget(), ix], &[&holder.owner]);
        self.close_proofs(&proofs);
        result
    }

    pub fn seize(&mut self, source: &Pubkey, destination: &Pubkey, amount: u64) -> TxResult {
        let ix = Instruction {
            program_id: PROGRAM_ID,
            accounts: accounts::Seize {
                source: *source,
                mint: self.mint.pubkey(),
                destination: *destination,
                permanent_delegate: self.issuer.pubkey(),
                token_program: t22new::id(),
            }
            .to_account_metas(None),
            data: instruction::Seize { amount }.data(),
        };
        let issuer = self.issuer.insecure_clone();
        self.send(&[ix], &[&issuer])
    }

    fn stage_proof<T, U>(&mut self, verify: ProofInstruction, proof: &T) -> Pubkey
    where
        T: Pod + ZkProofData<U>,
        U: Pod,
    {
        let context = Keypair::new();
        let space = std::mem::size_of::<ProofContextState<U>>();
        let create = solana_system_interface::instruction::create_account(
            &self.payer.pubkey(),
            &context.pubkey(),
            self.svm.minimum_balance_for_rent_exemption(space),
            space as u64,
            &zkif::id(),
        );
        self.send(&[create], &[&context])
            .expect("create proof context account");

        let payer = self.payer.pubkey();
        let context_pubkey = context.pubkey();
        let verify = verify.encode_verify_proof(
            Some(ContextStateInfo {
                context_state_account: &context_pubkey,
                context_state_authority: &payer,
            }),
            proof,
        );
        self.send(&[with_compute_budget(), verify], &[])
            .expect("verify proof");

        context_pubkey
    }

    fn close_proofs(&mut self, proofs: &[Pubkey]) {
        let payer = self.payer.pubkey();
        let closes: Vec<Instruction> = proofs
            .iter()
            .map(|proof| {
                close_context_state(
                    ContextStateInfo {
                        context_state_account: proof,
                        context_state_authority: &payer,
                    },
                    &payer,
                )
            })
            .collect();
        self.send(&closes, &[])
            .expect("close proof context accounts");
    }

    pub fn account_data(&self, address: &Pubkey) -> Vec<u8> {
        self.svm
            .get_account(address)
            .unwrap_or_else(|| panic!("account {address} missing"))
            .data
    }

    pub fn public_balance(&self, account: &Pubkey) -> u64 {
        let data = self.account_data(account);
        StateWithExtensions::<Account>::unpack(&data)
            .unwrap()
            .base
            .amount
    }

    pub fn is_frozen(&self, account: &Pubkey) -> bool {
        let data = self.account_data(account);
        StateWithExtensions::<Account>::unpack(&data)
            .unwrap()
            .base
            .is_frozen()
    }

    pub fn mint_data(&self) -> Vec<u8> {
        self.account_data(&self.mint.pubkey())
    }

    pub fn with_mint<R>(&self, read: impl FnOnce(&StateWithExtensions<Mint>) -> R) -> R {
        let data = self.mint_data();
        read(&StateWithExtensions::<Mint>::unpack(&data).unwrap())
    }

    pub fn confidential_account(&self, account: &Pubkey) -> ConfidentialTransferAccount {
        let data = self.account_data(account);
        *StateWithExtensions::<Account>::unpack(&data)
            .unwrap()
            .get_extension::<ConfidentialTransferAccount>()
            .expect("account is not configured for confidential transfers")
    }

    pub fn has_confidential_extension(&self, account: &Pubkey) -> bool {
        let data = self.account_data(account);
        StateWithExtensions::<Account>::unpack(&data)
            .unwrap()
            .get_extension::<ConfidentialTransferAccount>()
            .is_ok()
    }

    fn elgamal_pubkey_of(&self, account: &Pubkey) -> ElGamalPubkey {
        ElGamalPubkey::try_from(self.confidential_account(account).elgamal_pubkey).unwrap()
    }

    pub fn pending(&self, holder: &Holder) -> u64 {
        let ct = self.confidential_account(&holder.account);
        decrypt_lo_hi(
            &ct.pending_balance_lo,
            &ct.pending_balance_hi,
            PENDING_BALANCE_LO_BIT_LENGTH as usize,
            holder.elgamal.secret(),
        )
    }

    pub fn available(&self, holder: &Holder) -> u64 {
        let ct = self.confidential_account(&holder.account);
        decrypt(&ct.available_balance, holder.elgamal.secret())
    }

    pub fn decryptable_available(&self, holder: &Holder) -> u64 {
        let ct = self.confidential_account(&holder.account);
        let ciphertext = AeCiphertext::try_from(ct.decryptable_available_balance).unwrap();
        holder.aes.decrypt(&ciphertext).unwrap()
    }

    pub fn withheld_fee(&self, account: &Pubkey) -> u64 {
        let data = self.account_data(account);
        let state = StateWithExtensions::<Account>::unpack(&data).unwrap();
        let fee_amount = state
            .get_extension::<ConfidentialTransferFeeAmount>()
            .unwrap();
        decrypt(
            &fee_amount.withheld_amount,
            self.withdraw_withheld_authority.secret(),
        )
    }

    pub fn audit(&self, ciphertexts: &AuditorCiphertexts) -> u64 {
        decrypt_lo_hi(
            &ciphertexts.lo,
            &ciphertexts.hi,
            TRANSFER_AMOUNT_LO_BITS,
            self.auditor.secret(),
        )
    }
}

pub fn ata_address(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[owner.as_ref(), t22new::id().as_ref(), mint.as_ref()],
        &ASSOCIATED_TOKEN_PROGRAM_ID,
    )
    .0
}

pub fn expected_fee(amount: u64) -> u64 {
    let fee = (amount * FEE_BASIS_POINTS as u64).div_ceil(10_000);
    fee.min(MAXIMUM_FEE)
}

pub fn assert_fails(result: TxResult, expected: &str) {
    match result {
        Ok(()) => panic!("expected a failure mentioning {expected:?}"),
        Err(logs) => assert!(
            logs.to_lowercase().contains(&expected.to_lowercase()),
            "expected a failure mentioning {expected:?}, got:\n{logs}"
        ),
    }
}

fn with_compute_budget() -> Instruction {
    ComputeBudgetInstruction::set_compute_unit_limit(PROOF_COMPUTE_UNITS)
}

fn decrypt(ciphertext: &PodElGamalCiphertext, secret: &ElGamalSecretKey) -> u64 {
    ElGamalCiphertext::try_from(*ciphertext)
        .unwrap()
        .decrypt_u32(secret)
        .expect("balance too large to decrypt in a test")
}

fn decrypt_lo_hi(
    lo: &PodElGamalCiphertext,
    hi: &PodElGamalCiphertext,
    lo_bit_length: usize,
    secret: &ElGamalSecretKey,
) -> u64 {
    let combined = try_combine_lo_hi_ciphertexts(
        &ElGamalCiphertext::try_from(*lo).unwrap(),
        &ElGamalCiphertext::try_from(*hi).unwrap(),
        lo_bit_length,
    )
    .unwrap();
    combined
        .decrypt_u32(secret)
        .expect("balance too large to decrypt in a test")
}

fn pubkey_bytes(pubkey: &ElGamalPubkey) -> [u8; 32] {
    bytemuck::cast(PodElGamalPubkey::from(*pubkey))
}

fn ae_bytes(ciphertext: AeCiphertext) -> [u8; 36] {
    bytemuck::cast(PodAeCiphertext::from(ciphertext))
}
