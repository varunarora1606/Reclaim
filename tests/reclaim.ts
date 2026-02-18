import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Reclaim } from "../target/types/reclaim";
import { BN } from "bn.js";
import assert from "assert/strict";

describe("reclaim", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.reclaim as Program<Reclaim>;

  const SECONDS_PER_MINUTE = 60;
  const SECONDS_PER_HOUR = 60 * 60;
  const SECONDS_PER_DAY = 24 * 60 * 60;
  const LAMPORTS_PER_SOL = anchor.web3.LAMPORTS_PER_SOL;
  const inactivityPeriod = new BN(4);
  let rent: number;
  let escrowLastCheckIn: anchor.BN;
  const sleep = (ms: number) =>
    new Promise((resolve) => setTimeout(resolve, ms));

  const provider = anchor.getProvider();
  const tokenMint = anchor.web3.Keypair.generate();
  const escrowOwner = anchor.web3.Keypair.generate();
  const beneficiary = anchor.web3.Keypair.generate();

  const [globalPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("global_state")],
    program.programId,
  );
  const [solPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("sol_vault")],
    program.programId,
  );
  const [escrowPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("escrow_vault"), escrowOwner.publicKey.toBuffer()],
    program.programId,
  );
  const ownerAta = anchor.utils.token.associatedAddress({
    mint: tokenMint.publicKey,
    owner: escrowOwner.publicKey,
  });

  before(async () => {
    const sign = await provider.connection.requestAirdrop(
      escrowOwner.publicKey,
      10 * LAMPORTS_PER_SOL,
    );
    await provider.connection.confirmTransaction(sign);
    rent = await provider.connection.getMinimumBalanceForRentExemption(8);
  });

  it("Is initialized!", async () => {
    // Add your test here.
    const tx = await program.methods
      .initializeGlobalState()
      .accounts({
        payer: provider.wallet.publicKey,
        tokenMint: tokenMint.publicKey,
      })
      .signers([provider.wallet.payer, tokenMint])
      .rpc();
    console.log("Your transaction signature", tx);

    const global = await program.account.globalState.fetch(globalPda);
    const sol = await provider.connection.getAccountInfo(solPda);
    assert.ok(global.totalShares.eq(new BN(0)));
    assert.ok(global.tokenMint.equals(tokenMint.publicKey));
    assert.ok(global.solVault.equals(solPda));
    assert.equal(sol.lamports, rent);
  });

  it("Is escrow created!", async () => {
    const tx = await program.methods
      .createEscrow(inactivityPeriod)
      .accounts({
        owner: escrowOwner.publicKey,
        beneficiary: beneficiary.publicKey,
      })
      .signers([escrowOwner])
      .rpc();
    console.log("Your transaction signature", tx);

    const escrow = await program.account.escrowVault.fetch(escrowPda);
    assert.ok(escrow.beneficiary.equals(beneficiary.publicKey));
    assert.ok(escrow.owner.equals(escrowOwner.publicKey));
    assert.ok(escrow.shares.eq(new BN(0)));
    assert.ok(escrow.inactivityPeriod.eq(inactivityPeriod));
    assert.ok("active" in escrow.status);
    escrowLastCheckIn = escrow.lastCheckIn;
  });

  it("Is sol deposited!", async () => {
    await sleep(2000);
    const tx = await program.methods
      .depositSol(new BN(5 * LAMPORTS_PER_SOL))
      .accounts({
        owner: escrowOwner.publicKey,
        tokenMint: tokenMint.publicKey,
      })
      .signers([escrowOwner])
      .rpc();
    console.log("Your transaction signature", tx);

    const global = await program.account.globalState.fetch(globalPda);
    const sol = await provider.connection.getAccountInfo(solPda);
    const escrow = await program.account.escrowVault.fetch(escrowPda);
    const ata = await provider.connection.getTokenAccountBalance(ownerAta);
    assert.ok(global.totalShares.eq(new BN(5 * LAMPORTS_PER_SOL)));
    assert.equal(sol.lamports, 5 * LAMPORTS_PER_SOL + rent);
    assert.ok(escrow.shares.eq(new BN(5 * LAMPORTS_PER_SOL)));
    assert.ok(escrow.lastCheckIn.gt(escrowLastCheckIn));
    assert.equal(Number(ata.value.amount), 5 * LAMPORTS_PER_SOL);
    escrowLastCheckIn = escrow.lastCheckIn;
  });

  it("Is checked in!", async () => {
    await sleep(2000);
    const tx = await program.methods
      .checkIn()
      .accounts({
        owner: escrowOwner.publicKey,
      })
      .signers([escrowOwner])
      .rpc();
    console.log("Your transaction signature", tx);

    const escrow = await program.account.escrowVault.fetch(escrowPda);
    assert.ok(escrow.lastCheckIn.gt(escrowLastCheckIn));
    escrowLastCheckIn = escrow.lastCheckIn;
  });

  it("Is token redeemed!", async () => {
    const tx = await program.methods
      .redeemToken(new BN(1 * LAMPORTS_PER_SOL))
      .accounts({
        owner: escrowOwner.publicKey,
        tokenMint: tokenMint.publicKey,
      })
      .signers([escrowOwner])
      .rpc();
    console.log("Your transaction signature", tx);

    const global = await program.account.globalState.fetch(globalPda);
    const sol = await provider.connection.getAccountInfo(solPda);
    const ata = await provider.connection.getTokenAccountBalance(ownerAta);
    assert.ok(global.totalShares.eq(new BN(4 * LAMPORTS_PER_SOL)));
    assert.equal(sol.lamports, 4 * LAMPORTS_PER_SOL + rent);
    assert.equal(Number(ata.value.amount), 4 * LAMPORTS_PER_SOL);
  });

  it("Is inheritance claimed!", async () => {
    let failed = false
    try {
      await program.methods
        .claimInheritance()
        .accounts({
          escrowOwner: escrowOwner.publicKey,
          tokenMint: tokenMint.publicKey,
          beneficiary: beneficiary.publicKey,
        })
        .signers([beneficiary])
        .rpc();
    } catch (err) {
      failed = true
      const anchorErr = err as anchor.AnchorError;
      assert.equal(anchorErr.error.errorCode.code, "InactivityPeriodNotPassed");
    }
    assert.ok(failed)

    console.log("Hello");

    await sleep(4000);
    const tx = await program.methods
      .claimInheritance()
      .accounts({
        escrowOwner: escrowOwner.publicKey,
        tokenMint: tokenMint.publicKey,
        beneficiary: beneficiary.publicKey,
      })
      .signers([beneficiary])
      .rpc();
    console.log("Your transaction signature", tx);

    const global = await program.account.globalState.fetch(globalPda);
    const sol = await provider.connection.getAccountInfo(solPda);
    const beneficiaryBal = await provider.connection.getBalance(
      beneficiary.publicKey,
    );
    const escrow = await program.account.escrowVault.fetch(escrowPda);
    const ownerAtaBalance = await provider.connection.getTokenAccountBalance(
      ownerAta,
    );
    assert.ok(global.totalShares.eq(new BN(0)));
    assert.equal(sol.lamports, rent);
    assert.ok("finished" in escrow.status);
    assert.equal(Number(ownerAtaBalance.value.amount), 0);
    assert.equal(beneficiaryBal, 4 * LAMPORTS_PER_SOL);
  });
});
