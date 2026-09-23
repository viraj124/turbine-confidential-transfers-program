import { BN } from "@anchor-lang/core";
import {
  TOKEN_2022_PROGRAM_ID,
  createInitializeMintInstruction,
  getMintLen,
  getTransferFeeAmount,
} from "@solana/spl-token";
import { Keypair, SystemProgram } from "@solana/web3.js";
import { expect } from "chai";
import {
  DECIMALS,
  FEE_BASIS_POINTS,
  MAXIMUM_FEE,
  ONE_TOKEN,
  TestStablecoin,
  expectFailure,
} from "./helpers/stablecoin";

function expectedFee(amount: bigint): bigint {
  const fee = (amount * BigInt(FEE_BASIS_POINTS)) / 10_000n;
  return fee > MAXIMUM_FEE ? MAXIMUM_FEE : fee;
}

describe("transfer_with_fee", () => {
  let coin: TestStablecoin;

  beforeEach(async () => {
    coin = new TestStablecoin();
    await coin.create();
  });

  it("charges the epoch fee and withholds it in the destination", async () => {
    const alice = await coin.holder(100n * ONE_TOKEN);
    const bob = await coin.holder();
    const amount = 10n * ONE_TOKEN;
    const fee = expectedFee(amount);

    await coin.transfer(alice.account, bob.account, alice.owner, amount);

    expect(coin.readAccount(alice.account).amount).to.equal(
      100n * ONE_TOKEN - amount,
    );

    expect(coin.readAccount(bob.account).amount).to.equal(amount - fee);
    expect(
      getTransferFeeAmount(coin.readAccount(bob.account))?.withheldAmount,
    ).to.equal(fee);
    expect(fee > 0n, "fee should not round to zero").to.be.true;
  });

  it("caps the fee at the mint's maximum", async () => {
    const alice = await coin.holder(10_000n * ONE_TOKEN);
    const bob = await coin.holder();
    const amount = 5_000n * ONE_TOKEN;

    await coin.transfer(alice.account, bob.account, alice.owner, amount);

    expect(coin.readAccount(bob.account).amount).to.equal(amount - MAXIMUM_FEE);
  });

  it("charges nothing on a zero-fee mint", async () => {
    const free = new TestStablecoin();
    await free.create(0, 0n);
    const alice = await free.holder(10n * ONE_TOKEN);
    const bob = await free.holder();

    await free.transfer(alice.account, bob.account, alice.owner, ONE_TOKEN);

    expect(free.readAccount(bob.account).amount).to.equal(ONE_TOKEN);
  });

  it("rounds the fee up, so tiny transfers are not free", async () => {
    const alice = await coin.holder(ONE_TOKEN);
    const bob = await coin.holder();
    const amount = 101n;

    await coin.transfer(alice.account, bob.account, alice.owner, amount);

    expect(coin.readAccount(bob.account).amount).to.equal(99n);
  });

  it("refuses a transfer larger than the balance", async () => {
    const alice = await coin.holder(ONE_TOKEN);
    const bob = await coin.holder();

    await expectFailure(
      () =>
        coin.transfer(alice.account, bob.account, alice.owner, 2n * ONE_TOKEN),
      "insufficient funds",
    );
  });

  it("refuses an authority that does not own the source", async () => {
    const alice = await coin.holder(10n * ONE_TOKEN);
    const bob = await coin.holder();

    await expectFailure(
      () => coin.transfer(alice.account, bob.account, bob.owner, ONE_TOKEN),
      "owner does not match",
    );
  });

  it("rejects a mint that carries no transfer fee config", async () => {
    const coin = new TestStablecoin();
    await coin.create();
    const alice = await coin.holder(10n * ONE_TOKEN);
    const bob = await coin.holder();

    const plainMint = Keypair.generate();
    const space = getMintLen([]);
    coin.send(
      [
        SystemProgram.createAccount({
          fromPubkey: coin.provider.publicKey,
          newAccountPubkey: plainMint.publicKey,
          space,
          lamports: Number(
            coin.provider.svm.minimumBalanceForRentExemption(BigInt(space)),
          ),
          programId: TOKEN_2022_PROGRAM_ID,
        }),
        createInitializeMintInstruction(
          plainMint.publicKey,
          DECIMALS,
          coin.issuer.publicKey,
          null,
          TOKEN_2022_PROGRAM_ID,
        ),
      ],
      [plainMint],
    );

    await expectFailure(
      () =>
        coin.program.methods
          .transferWithFee(new BN(1))
          .accounts({
            source: alice.account,
            mint: plainMint.publicKey,
            destination: bob.account,
            authority: alice.owner.publicKey,
          })
          .signers([alice.owner])
          .rpc(),
      "mint has no transfer fee config",
    );
  });

  it("refuses an account owned by another program in place of the mint", async () => {
    const alice = await coin.holder(10n * ONE_TOKEN);
    const bob = await coin.holder();

    await expectFailure(
      () =>
        coin.program.methods
          .transferWithFee(new BN(1))
          .accounts({
            source: alice.account,
            mint: alice.owner.publicKey,
            destination: bob.account,
            authority: alice.owner.publicKey,
          })
          .signers([alice.owner])
          .rpc(),
      "ConstraintOwner",
    );
  });
});
