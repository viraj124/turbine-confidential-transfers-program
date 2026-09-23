# Confidential stablecoin

An Anchor program that issues a remittance stablecoin on **Token-2022**, first as a public mint and then re-issued with **confidential transfers** and a **seizure authority**.

The public mint charges a protocol fee on every transfer, freezes every new account until the holder clears KYC, stores its own metadata on-chain, and can be closed once decommissioned.
The re-issued mint keeps all of that and adds two requirements that pull in opposite directions: regulators want the issuer able to seize funds from sanctioned wallets, and users want transfer amounts hidden.
The program implements both, and the tests show exactly where they stop meeting.

```
create_stablecoin ─────────▶ [ public mint ] ──▶ transfer_with_fee      (holder, fee withheld)
                                   │
create_confidential_stablecoin ─▶ [ confidential mint ]
                                   │
                                   ├──▶ thaw_after_kyc                 (issuer, per account)
                                   ├──▶ seize                          (issuer, public balance only)
                                   │
                                   └──▶ configure ─▶ approve ─▶ deposit ─▶ apply ─▶ transfer ─▶ withdraw
                                        (owner)     (issuer)    (owner)    (owner)  (owner)     (owner)
```

## The mint

`create_stablecoin` stacks four extensions onto one mint:

| extension             | why                                                                                            |
| --------------------- | ---------------------------------------------------------------------------------------------- |
| `TransferFeeConfig`   | issuer revenue: a cut of every transfer, withheld in the recipient's account                   |
| `MetadataPointer`     | points at the mint itself, so wallets read name and symbol from the account they already trust |
| `DefaultAccountState` | `Frozen`, so every new token account is locked until the issuer thaws it after KYC             |
| `MintCloseAuthority`  | lets the issuer close the mint and reclaim its rent once supply is zero                        |

The issuer holds every authority: mint, freeze, fee config, fee withdrawal, metadata and close.

Token-2022 fixes the order of creation:

```
create_account -> every extension initializer -> initialize_mint2 -> token_metadata_initialize
```

- **Extensions come before `initialize_mint2`.**
  Each extension initializer only works on an uninitialized mint, and `initialize_mint2` seals the set for good.
  It also checks the set as a whole, for example rejecting a frozen-by-default mint with no freeze authority.
- **Metadata comes after it.**
  Writing `TokenMetadata` needs the mint authority's signature, and there is no mint authority until `initialize_mint2` sets one.
- **The account is sized in two parts.**
  `ExtensionType::try_calculate_account_len` gives the exact length of the four fixed-size extensions, and that is all `create_account` allocates, because `initialize_mint2` rejects trailing bytes it cannot account for.
  Metadata is variable length, so its rent is paid up front and Token-2022 grows the account itself when the metadata is written.

## The fee

`transfer_with_fee` uses `transfer_checked_with_fee`, which states the fee in the instruction and makes Token-2022 fail the transfer if its own calculation differs.
The fee comes from the mint's live config via `calculate_epoch_fee(current_epoch, amount)`, never a cached rate, because a fee change is scheduled as `newer_transfer_fee` and only takes effect at a later epoch.

Worked examples at a 1% fee capped at 5 tokens:

| transfer     | fee                 | recipient receives |
| ------------ | ------------------- | ------------------ |
| 10 tokens    | 0.1 tokens          | 9.9 tokens         |
| 101 units    | 2 units (rounds up) | 99 units           |
| 5,000 tokens | 5 tokens (capped)   | 4,995 tokens       |

The sender is always debited the full amount.
The fee sits withheld in the recipient's account until the issuer harvests it.

## KYC

`DefaultAccountState = Frozen` freezes the **token account**, not the wallet.
A wallet can still send SOL and other tokens, it just cannot move this coin until its account is thawed.

`thaw_after_kyc` has the freeze authority thaw one account.
It never touches the mint's default state, so every account created afterwards is still born frozen and has to clear KYC on its own.

## Reading state

Every read of mint or account data goes through `StateWithExtensions`, never a raw `Mint::unpack`.
The extension data sits after the base state, so a raw unpack either rejects the longer account or silently drops the extensions.

Accounts the program reads are taken as `UncheckedAccount` with an `owner = token_program` constraint.
`InterfaceAccount<Mint>` would parse the extensions and then throw them away, and `StateWithExtensions::unpack` only checks the byte layout, so without the owner check a forged account from another program with a 0% fee would pass.
The reads live in `token_state.rs` and each one returns before the CPI, because a CPI fails while a borrow of an account it writes is still open.

## The confidential mint

`create_confidential_stablecoin` is a new mint, not an upgrade, because `ConfidentialTransferMint` cannot be added after creation.
It carries forward the original four extensions and adds three:

| extension                       | why                                                                                              |
| ------------------------------- | ------------------------------------------------------------------------------------------------ |
| `PermanentDelegate`             | the issuer can move or burn tokens from any account of the mint, without the holder's signature  |
| `ConfidentialTransferMint`      | holders can hide balances and amounts; `auto_approve_new_accounts = false` makes approval manual |
| `ConfidentialTransferFeeConfig` | required by Token-2022 whenever a mint has both a transfer fee and confidential transfers        |

The last one is not a choice.
A fee on a hidden amount has to be hidden too, so Token-2022 withholds it encrypted under `withdraw_withheld_authority_elgamal_pubkey`, and rejects the mint at `initialize_mint2` without it.

`auditor_elgamal_pubkey` is optional.
If set, its holder can decrypt the amount of every confidential transfer on the mint, but not balances, which are encrypted only for their owner.

## The confidential lifecycle

A configured token account holds three balances:

```
public ──deposit──▶ pending ──apply──▶ available ──transfer──▶ recipient's pending
  ▲                                        │
  └────────────────withdraw────────────────┘
```

- **Public** is the ordinary balance, visible to everyone.
- **Pending** is where deposits and incoming transfers land. It cannot be spent.
- **Available** is what the owner can transfer or withdraw.

Worked example, from the lifecycle test at a 1% fee:

| step                     | Alice public | Alice available | Bob pending | Bob available | Bob public |
| ------------------------ | ------------ | --------------- | ----------- | ------------- | ---------- |
| start                    | 100          | 0               | 0           | 0             | 0          |
| Alice deposits 100       | 0            | 0 (100 pending) | 0           | 0             | 0          |
| Alice applies            | 0            | 100             | 0           | 0             | 0          |
| Alice sends 40 (0.4 fee) | 0            | 60              | 39.6        | 0             | 0          |
| Bob applies              | 0            | 60              | 0           | 39.6          | 0          |
| Bob withdraws 39.6       | 0            | 60              | 0           | 0             | 39.6       |

Each holder applies their own pending balance, and only the account owner can.
The split exists so a sender can never change the balance a transfer or withdrawal proof was generated against: incoming credits wait in pending until the owner folds them in.

`withdraw_confidential` draws on the available balance only, so funds that were just deposited or received have to be applied first.
The program leaves that ordering to the client rather than requiring an empty pending balance, because such a check would let anyone block a withdrawal by sending the holder a confidential transfer.

**Configure is owner-only, creation is not.**
Anyone can create an ATA for anyone, because all it holds is an empty public balance.
`configure_confidential_account` registers the ElGamal public key every hidden amount sent to the account will be encrypted under, so only the owner may do it.
If anyone else could, they could register their own key and read or strand the owner's funds.
Configuring reallocates the account first, because confidential transfers are opt in and an ATA is created without room for them.
Under the manual policy the configured account then waits for `approve_confidential_account` from the issuer.

**Each balance is stored twice.**
An ElGamal ciphertext the program can do arithmetic on without decrypting, and an AE ciphertext only the owner can produce and decrypt instantly.
That is why `apply_pending_balance` and every debit take a new AE balance as an argument: the program has no key to compute it.
`expected_pending_balance_credit_counter` tells Token-2022 how many credits that AE balance accounts for, so a transfer that lands in between is left pending for next time.

### Proofs

Token-2022 cannot see the amounts it moves, so every debit carries zero-knowledge proofs.
The client generates them off-chain with its secret keys, the ZK ElGamal proof program verifies each one into a context state account, and the program passes those accounts to Token-2022, which reads the verified results.
A transfer on a fee mint needs five, and together they are far too large to travel with the transfer itself.

| proof                    | proves                                                                      | stops                                                    |
| ------------------------ | --------------------------------------------------------------------------- | -------------------------------------------------------- |
| equality                 | the new available balance matches the commitment the range proof covers     | swapping in a different balance                          |
| transfer amount validity | the amount is encrypted identically for sender, recipient and auditor       | debiting 10 while crediting 100, or lying to the auditor |
| fee sigma                | the hidden fee is the mint's rate applied to the hidden amount, capped      | paying a smaller fee                                     |
| fee validity             | the fee is encrypted correctly for the recipient and the withheld authority | a fee the issuer can never decrypt or collect            |
| range                    | the amount, the fee and the remaining balance are all non-negative          | spending more than the balance, since ciphertexts wrap   |

Configure needs one proof (pubkey validity), withdraw needs two (equality and range), and deposit and apply need none.

## The gap

The permanent delegate can only reach an account's **public** balance.
Confidential balances are ElGamal ciphertexts under the holder's key, and Token-2022 gives the permanent delegate no instruction that debits them.

So a sanctioned holder who deposits into the confidential balance before the issuer acts puts those funds permanently out of reach of seizure.
Pending and available balances are both protected, and nothing the issuer holds changes that:

- **Freezing** the account blocks every confidential operation, so the holder cannot move or withdraw the funds either.
  It stops the funds, it does not recover them.
  A frozen account also cannot be debited by anyone, so even seizing its public balance needs a thaw first, in the same transaction.
- **The auditor key** lets the issuer see the amount of every confidential transfer, so it knows what moved and where.
  Seeing is not seizing.
- **Manual approval** is the only preventive control.
  The issuer can refuse to approve an account for confidential transfers at all, but once approved and funded, the confidential balance is beyond the permanent delegate.

The practical consequence for the issuer: freeze first, the moment an account is sanctioned, and treat approval as a compliance decision rather than a formality.

## Instructions

|                                                                                                                                                 | Signer             | Effect                                                                                                    |
| ----------------------------------------------------------------------------------------------------------------------------------------------- | ------------------ | --------------------------------------------------------------------------------------------------------- |
| `create_stablecoin(decimals, transfer_fee_basis_points, maximum_fee, name, symbol, uri)`                                                        | issuer, mint       | Creates the public mint with its four extensions and on-chain metadata.                                   |
| `transfer_with_fee(amount)`                                                                                                                     | owner or delegate  | Transfers with the current epoch's fee stated up front. `MissingTransferFeeConfig` on a mint without one. |
| `thaw_after_kyc()`                                                                                                                              | freeze authority   | Thaws one token account. The mint's default state is untouched.                                           |
| `create_confidential_stablecoin(..., withdraw_withheld_authority_elgamal_pubkey, auditor_elgamal_pubkey)`                                       | issuer, mint       | Creates the confidential mint: the original four extensions plus the three above.                         |
| `seize(amount)`                                                                                                                                 | permanent delegate | Moves tokens out of any account without the holder's signature. Reaches the public balance only.          |
| `configure_confidential_account(decryptable_zero_balance)`                                                                                      | owner              | Reallocates the account and registers the owner's ElGamal key. Takes a pubkey validity proof account.     |
| `approve_confidential_account()`                                                                                                                | issuer             | Lets a configured account use confidential balances.                                                      |
| `deposit_confidential(amount)`                                                                                                                  | owner              | Public to pending. No proof needed, the amount was already public.                                        |
| `apply_pending_balance(expected_pending_balance_credit_counter, new_decryptable_available_balance)`                                             | owner              | Pending to available.                                                                                     |
| `confidential_transfer(new_source_decryptable_available_balance, transfer_amount_auditor_ciphertext_lo, transfer_amount_auditor_ciphertext_hi)` | owner              | Available to the recipient's pending, fee withheld encrypted. Takes five proof accounts.                  |
| `withdraw_confidential(amount, new_decryptable_available_balance)`                                                                              | owner              | Available to public. Takes equality and range proof accounts.                                             |

## Accounts

The program keeps no state of its own. Everything lives in Token-2022 accounts.

```
Public mint (keypair)
  base Mint                      decimals, supply, mint + freeze authority = issuer
  TransferFeeConfig              rate, cap, config + withdraw withheld authority = issuer
  MetadataPointer                authority = issuer, metadata address = the mint itself
  DefaultAccountState            Frozen
  MintCloseAuthority             issuer
  TokenMetadata                  name, symbol, uri (variable length, written last)

Confidential mint (keypair)
  everything above, plus
  PermanentDelegate              issuer
  ConfidentialTransferMint       authority = issuer, auto approve = false, optional auditor key
  ConfidentialTransferFeeConfig  authority = issuer, withdraw withheld ElGamal key

Token account (ATA, created by anyone)
  base Account                   owner, amount, state (born Frozen)
  TransferFeeAmount              public fees withheld here
  ConfidentialTransferAccount    added by configure: ElGamal key, approved, pending lo/hi,
                                 available, decryptable available, credit counters
  ConfidentialTransferFeeAmount  added by configure: confidential fees withheld here
```

## Tests

Both suites run against [LiteSVM](https://github.com/LiteSVM/litesvm), an in-process Solana runtime, so there is no validator to start and nothing is deployed.
Each harness boots its own LiteSVM instance, so tests never share state unless they deliberately reuse one harness, as the read-only mint checks in `create-stablecoin.ts` do.

```bash
bun install
bun run test
```

`bun run test` builds the program with `anchor build`, then runs the TypeScript suite and the Rust suite.
`bun run test:ts` and `bun run test:rust` run one suite each against the existing build.
Note `bun run test`, not `bun test` - the latter invokes Bun's own test runner, which does not understand a Mocha suite.

**TypeScript, for the public mint** (`tests/`).
`tests/helpers/litesvm-provider.ts` is an Anchor `Provider` that converts each `@solana/web3.js` transaction with `@solana/compat` and executes it in LiteSVM, so the usual `program.methods...rpc()` calls work unchanged.
`tests/helpers/stablecoin.ts` holds the `TestStablecoin` harness.
`create-stablecoin.ts`, `transfer-with-fee.ts` and `thaw-after-kyc.ts` cover the extensions and sizing, the fee maths including rounding and the cap, and the KYC gate.
The suite runs through `ts-mocha --type-check`, so every test is type checked against the IDL types.

**Rust, for the confidential mint** (`programs/confidential_transfers/tests/`).
Confidential transfers need client-side ZK proofs, and the TypeScript ZK SDK cannot build them for a fee mint: it has no arithmetic on ElGamal ciphertexts and no scalar multiplication on commitments, and a transfer needs both.
The Rust `spl-token-confidential-transfer-proof-generation` crate does, so this suite is written in Rust.
`tests/common/mod.rs` holds the `TestCoin` harness, which plays the client: it generates each proof, verifies it into a context state account, calls the instruction, then closes the proof accounts to reclaim their rent.
`tests/confidential.rs` covers the re-issued mint, owner-only configure, manual approval, the full lifecycle with balances decrypted at every step, the auditor and fee keys, and the seizure gap.

One detail matters to any client reading these balances.
The pending balance is stored as a low and a high half, and an incoming transfer's fee is subtracted from the low half alone, which can take it below zero on its own.
The halves have to be combined before decrypting, never decrypted one at a time.

Anchor 1.2.0 appears in three places that have to move together: `anchor-lang` and `anchor-spl` in `programs/confidential_transfers/Cargo.toml`, and `@anchor-lang/core` in `package.json`.
