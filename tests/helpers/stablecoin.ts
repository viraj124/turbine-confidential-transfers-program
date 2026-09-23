import { BN, Program } from "@anchor-lang/core";
import {
  Account,
  ExtensionType,
  Mint,
  TOKEN_2022_PROGRAM_ID,
  createAssociatedTokenAccountInstruction,
  createMintToInstruction,
  getAssociatedTokenAddressSync,
  unpackAccount,
  unpackMint,
} from "@solana/spl-token";
import {
  Keypair,
  PublicKey,
  Signer,
  SystemProgram,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";
import { expect } from "chai";
import * as path from "path";
import idl from "../../target/idl/confidential_transfers.json";
import { ConfidentialTransfers } from "../../target/types/confidential_transfers";
import { LiteSVMProvider, LiteSVMTransactionError } from "./litesvm-provider";

export const DECIMALS = 6;
export const ONE_TOKEN = 1_000_000n;
export const FEE_BASIS_POINTS = 100;
export const MAXIMUM_FEE = 5n * ONE_TOKEN;
export const NAME = "Remittance USD";
export const SYMBOL = "RUSD";
export const URI = "https://example.com/rusd.json";

export const PROGRAM_ID = new PublicKey(idl.address);
export const MINT_EXTENSIONS = [
  ExtensionType.TransferFeeConfig,
  ExtensionType.MetadataPointer,
  ExtensionType.DefaultAccountState,
  ExtensionType.MintCloseAuthority,
];

const PROGRAM_PATH = path.join(
  __dirname,
  "../../target/deploy/confidential_transfers.so",
);

export class TestStablecoin {
  readonly provider = new LiteSVMProvider();
  readonly program = new Program<ConfidentialTransfers>(
    idl as ConfidentialTransfers,
    this.provider,
  );
  readonly issuer = Keypair.generate();
  readonly mintKeypair = Keypair.generate();

  get mint(): PublicKey {
    return this.mintKeypair.publicKey;
  }

  constructor() {
    this.provider.addProgram(PROGRAM_ID, PROGRAM_PATH);
    this.provider.airdrop(this.issuer.publicKey, 1_000_000_000_000n);
  }

  async create(
    feeBasisPoints = FEE_BASIS_POINTS,
    maximumFee = MAXIMUM_FEE,
  ): Promise<void> {
    await this.program.methods
      .createStablecoin(
        DECIMALS,
        feeBasisPoints,
        new BN(maximumFee.toString()),
        NAME,
        SYMBOL,
        URI,
      )
      .accounts({
        issuer: this.issuer.publicKey,
        mint: this.mint,
      })
      .signers([this.issuer, this.mintKeypair])
      .rpc();
  }


  createTokenAccount(owner: PublicKey): PublicKey {
    const ata = getAssociatedTokenAddressSync(
      this.mint,
      owner,
      true,
      TOKEN_2022_PROGRAM_ID,
    );

    this.send([
      createAssociatedTokenAccountInstruction(
        this.provider.publicKey,
        ata,
        owner,
        this.mint,
        TOKEN_2022_PROGRAM_ID,
      ),
    ]);

    return ata;
  }

  async thaw(
    tokenAccount: PublicKey,
    freezeAuthority: Keypair = this.issuer,
  ): Promise<void> {
    await this.program.methods
      .thawAfterKyc()
      .accounts({
        tokenAccount,
        mint: this.mint,
        freezeAuthority: freezeAuthority.publicKey,
      })
      .signers([freezeAuthority])
      .rpc();
  }

  mintTo(tokenAccount: PublicKey, amount: bigint): void {
    this.send(
      [
        createMintToInstruction(
          this.mint,
          tokenAccount,
          this.issuer.publicKey,
          amount,
          [],
          TOKEN_2022_PROGRAM_ID,
        ),
      ],
      [this.issuer],
    );
  }

  async transfer(
    source: PublicKey,
    destination: PublicKey,
    authority: Keypair,
    amount: bigint,
  ): Promise<void> {
    await this.program.methods
      .transferWithFee(new BN(amount.toString()))
      .accounts({
        source,
        mint: this.mint,
        destination,
        authority: authority.publicKey,
      })
      .signers([authority])
      .rpc();
  }

  async holder(amount = 0n): Promise<{ owner: Keypair; account: PublicKey }> {
    const owner = Keypair.generate();
    this.provider.airdrop(owner.publicKey, 1_000_000_000n);

    const account = this.createTokenAccount(owner.publicKey);
    await this.thaw(account);
    if (amount > 0n) {
      this.mintTo(account, amount);
    }

    return { owner, account };
  }

  readMint(): Mint {
    const info = this.provider.getAccountInfo(this.mint);
    expect(info, "mint account missing").to.not.be.null;
    return unpackMint(this.mint, info, TOKEN_2022_PROGRAM_ID);
  }

  readMintData(): Buffer {
    const info = this.provider.getAccountInfo(this.mint);
    expect(info, "mint account missing").to.not.be.null;
    return info!.data;
  }

  readMintLamports(): number {
    const info = this.provider.getAccountInfo(this.mint);
    expect(info, "mint account missing").to.not.be.null;
    return info!.lamports;
  }

  readAccount(address: PublicKey): Account {
    const info = this.provider.getAccountInfo(address);
    expect(info, "token account missing").to.not.be.null;
    return unpackAccount(address, info, TOKEN_2022_PROGRAM_ID);
  }

  send(instructions: TransactionInstruction[], signers: Signer[] = []): void {
    this.provider.sendSync(new Transaction().add(...instructions), signers);
  }
}

export async function expectFailure(
  action: () => unknown | Promise<unknown>,
  expected: string,
): Promise<void> {
  try {
    await action();
  } catch (error) {
    // Anchor translates a failed `.rpc()` into an AnchorError or a
    // ProgramError, so the logs can arrive on any of several shapes.
    const logs =
      error instanceof LiteSVMTransactionError
        ? error.logs
        : ((error as { logs?: string[] }).logs ?? []);
    expect([String(error), ...logs].join("\n")).to.include(expected);
    return;
  }

  expect.fail(`expected a failure mentioning "${expected}"`);
}
