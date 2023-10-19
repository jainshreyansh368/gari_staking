import { web3, Program, BN } from "@coral-xyz/anchor";
import { MandateContract } from "../target/types/mandate_contract";

export const initPlatform = async (
  program: Program<MandateContract>,
  admin: web3.Keypair,
  platformPda: web3.PublicKey,
  payer: web3.Keypair
) => {
  await program.methods
    .initPlatform(new BN(12), new BN(12), new BN(12))
    .accounts({
      platformState: platformPda,
      admin: admin.publicKey,
      payer: payer.publicKey,
      systemProgram: web3.SystemProgram.programId,
    })
    .signers([admin])
    .rpc();

  const platform = await program.account.platformData.fetch(platformPda);
  return platform;
};
