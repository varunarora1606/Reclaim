import * as anchor from "@coral-xyz/anchor";
import * as fs from "fs";
import { Reclaim } from "../target/types/reclaim";
import * as dotenv from "dotenv";

dotenv.config()

async function main() {
  const rpcUrl = process.env.RPC_URL;
  if (!rpcUrl) throw new Error("RPC_URL is not set in .env");

  const connection = new anchor.web3.Connection(
  rpcUrl,
  "confirmed"
);

  const adminKeypayer = anchor.web3.Keypair.fromSecretKey(
    Uint8Array.from(
      JSON.parse(
        fs.readFileSync(`${process.env.HOME}/.config/solana/id.json`, "utf-8"),
      ),
    ),
  );

  const wallet = new anchor.Wallet(adminKeypayer);
  const provider = new anchor.AnchorProvider(connection, wallet, {});
  anchor.setProvider(provider);

  const program = anchor.workspace.reclaim as anchor.Program<Reclaim>;

  const tokenMint = anchor.web3.Keypair.generate();

  console.log("Token Mint:", tokenMint.publicKey.toBase58());

  const tx = await program.methods
    .initializeGlobalState()
    .accounts({
      payer: adminKeypayer.publicKey,
      tokenMint: tokenMint.publicKey,
    })
    .signers([adminKeypayer, tokenMint])
    .rpc();

  console.log("Initialized! TX:", tx);

  fs.writeFileSync(
    "scripts/deployment.json",
    JSON.stringify({
      tokenMint: tokenMint.publicKey.toBase58(),
      programId: program.programId.toBase58(),
    }, null, 2),
  );
}

main();
