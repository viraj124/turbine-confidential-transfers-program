import { AccountState, getDefaultAccountState } from "@solana/spl-token";
import { Keypair } from "@solana/web3.js";
import { expect } from "chai";
import { ONE_TOKEN, TestStablecoin, expectFailure } from "./helpers/stablecoin";

describe("thaw_after_kyc", () => {
  let coin: TestStablecoin;

  beforeEach(async () => {
    coin = new TestStablecoin();
    await coin.create();
  });

  it("thaws the one account that cleared KYC", async () => {
    const owner = Keypair.generate();
    const account = coin.createTokenAccount(owner.publicKey);
    expect(coin.readAccount(account).isFrozen, "born frozen").to.be.true;

    await coin.thaw(account);

    expect(coin.readAccount(account).isFrozen).to.be.false;
  });

  it("leaves the mint's default state alone, so later accounts stay frozen", async () => {
    const first = coin.createTokenAccount(Keypair.generate().publicKey);
    await coin.thaw(first);

    const second = coin.createTokenAccount(Keypair.generate().publicKey);

    expect(coin.readAccount(second).isFrozen, "still born frozen").to.be.true;
    expect(getDefaultAccountState(coin.readMint())?.state).to.equal(
      AccountState.Frozen,
    );
  });

  it("lets a thawed account receive and send tokens", async () => {
    const alice = await coin.holder(10n * ONE_TOKEN);
    const bob = await coin.holder();

    await coin.transfer(alice.account, bob.account, alice.owner, ONE_TOKEN);

    expect(coin.readAccount(bob.account).amount > 0n).to.be.true;
  });

  it("refuses anyone but the freeze authority", async () => {
    const impostor = Keypair.generate();
    coin.provider.airdrop(impostor.publicKey, 1_000_000_000n);
    const account = coin.createTokenAccount(Keypair.generate().publicKey);

    await expectFailure(
      () => coin.thaw(account, impostor),
      "owner does not match",
    );

    expect(coin.readAccount(account).isFrozen, "still frozen").to.be.true;
  });

  it("refuses an account that is already thawed", async () => {
    const account = coin.createTokenAccount(Keypair.generate().publicKey);
    await coin.thaw(account);

    await expectFailure(() => coin.thaw(account), "Invalid account state");
  });

  it("blocks transfers until KYC clears", async () => {
    const alice = await coin.holder(10n * ONE_TOKEN);
    const bob = coin.createTokenAccount(Keypair.generate().publicKey);

    await expectFailure(
      () => coin.transfer(alice.account, bob, alice.owner, ONE_TOKEN),
      "Account is frozen",
    );

    await coin.thaw(bob);
    await coin.transfer(alice.account, bob, alice.owner, ONE_TOKEN);

    expect(coin.readAccount(bob).amount > 0n).to.be.true;
  });
});
