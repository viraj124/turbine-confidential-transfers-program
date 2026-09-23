import {
  AccountState,
  ExtensionType,
  LENGTH_SIZE,
  TYPE_SIZE,
  getDefaultAccountState,
  getExtensionData,
  getExtensionTypes,
  getMetadataPointerState,
  getMintCloseAuthority,
  getMintLen,
  getTransferFeeConfig,
} from "@solana/spl-token";
import { TokenMetadata, pack, unpack } from "@solana/spl-token-metadata";
import { Keypair, PublicKey } from "@solana/web3.js";
import { expect } from "chai";
import {
  DECIMALS,
  FEE_BASIS_POINTS,
  MAXIMUM_FEE,
  MINT_EXTENSIONS,
  NAME,
  SYMBOL,
  TestStablecoin,
  URI,
} from "./helpers/stablecoin";

describe("create_stablecoin", () => {
  let coin: TestStablecoin;

  before(async () => {
    coin = new TestStablecoin();
    await coin.create();
  });

  it("stacks the four extensions, plus the metadata written afterwards", () => {
    const mint = coin.readMint();
    const extensions = getExtensionTypes(mint.tlvData);

    for (const extension of MINT_EXTENSIONS) {
      expect(extensions, ExtensionType[extension]).to.include(extension);
    }
    expect(extensions).to.include(ExtensionType.TokenMetadata);
  });

  it("is sized by try_calculate_account_len, with room for the metadata", () => {
    const metadata: TokenMetadata = {
      updateAuthority: coin.issuer.publicKey,
      mint: coin.mint,
      name: NAME,
      symbol: SYMBOL,
      uri: URI,
      additionalMetadata: [],
    };
    const fixedLen = getMintLen(MINT_EXTENSIONS);
    const metadataLen = TYPE_SIZE + LENGTH_SIZE + pack(metadata).length;

    expect(coin.readMintData().length).to.equal(fixedLen + metadataLen);
  });

  it("is funded for the metadata it grows into, not just the fixed length", () => {
    const length = coin.readMintData().length;
    const rentExempt = coin.provider.svm.minimumBalanceForRentExemption(
      BigInt(length),
    );

    expect(
      BigInt(coin.readMintLamports()) >= rentExempt,
      "mint is not rent exempt at its grown length",
    ).to.be.true;
  });

  it("points the metadata pointer at the mint itself", () => {
    const pointer = getMetadataPointerState(coin.readMint());

    expect(pointer?.metadataAddress?.toBase58()).to.equal(coin.mint.toBase58());
    expect(pointer?.authority?.toBase58()).to.equal(
      coin.issuer.publicKey.toBase58(),
    );
  });

  it("stores name, symbol and uri on the mint itself", () => {
    const data = getExtensionData(
      ExtensionType.TokenMetadata,
      coin.readMint().tlvData,
    );
    expect(data, "no metadata on the mint").to.not.be.null;

    const metadata = unpack(data!);
    expect(metadata.name).to.equal(NAME);
    expect(metadata.symbol).to.equal(SYMBOL);
    expect(metadata.uri).to.equal(URI);
    expect(metadata.mint.toBase58()).to.equal(coin.mint.toBase58());
  });

  it("charges the configured transfer fee, with the issuer as authority", () => {
    const config = getTransferFeeConfig(coin.readMint());

    expect(config?.newerTransferFee.transferFeeBasisPoints).to.equal(
      FEE_BASIS_POINTS,
    );
    expect(config?.newerTransferFee.maximumFee).to.equal(MAXIMUM_FEE);
    expect(config?.transferFeeConfigAuthority?.toBase58()).to.equal(
      coin.issuer.publicKey.toBase58(),
    );
    expect(config?.withdrawWithheldAuthority?.toBase58()).to.equal(
      coin.issuer.publicKey.toBase58(),
    );
  });

  it("freezes new accounts by default and keeps a freeze authority", () => {
    const mint = coin.readMint();

    expect(getDefaultAccountState(mint)?.state).to.equal(AccountState.Frozen);
    expect(mint.freezeAuthority?.toBase58()).to.equal(
      coin.issuer.publicKey.toBase58(),
    );
    expect(mint.decimals).to.equal(DECIMALS);
    expect(mint.mintAuthority?.toBase58()).to.equal(
      coin.issuer.publicKey.toBase58(),
    );
  });

  it("can be closed by the issuer once decommissioned", () => {
    const closeAuthority = getMintCloseAuthority(coin.readMint());

    expect(closeAuthority?.closeAuthority?.toBase58()).to.equal(
      coin.issuer.publicKey.toBase58(),
    );
  });

  it("creates every token account frozen, whoever pays for it", () => {
    const holder = Keypair.generate();
    const account = coin.createTokenAccount(holder.publicKey);

    expect(coin.readAccount(account).isFrozen).to.be.true;
  });

  it("rejects a second mint at the same address", async () => {
    let failed = false;
    try {
      await coin.create();
    } catch {
      failed = true;
    }

    expect(failed, "creating the same mint twice should fail").to.be.true;
  });
});


describe("mint sizing", () => {
  it("counts every fixed extension", () => {
    const base = getMintLen([]);
    const withExtensions = getMintLen(MINT_EXTENSIONS);

    expect(withExtensions).to.be.greaterThan(base);
  });

  it("does not count TokenMetadata, which is variable length", () => {
    expect(() =>
      getMintLen([...MINT_EXTENSIONS, ExtensionType.TokenMetadata]),
    ).to.throw();
  });
});

describe("program id", () => {
  it("matches declare_id", () => {
    const coin = new TestStablecoin();
    expect(coin.program.programId.toBase58()).to.equal(
      new PublicKey(coin.program.idl.address).toBase58(),
    );
  });
});
