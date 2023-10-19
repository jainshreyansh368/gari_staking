import { BN, Program, web3 } from "@coral-xyz/anchor";
import { MandateContract } from "../target/types/mandate_contract";

export const updatePlatform = async (
  program: Program<MandateContract>,
  admin: web3.Keypair,
  one: BN,
  two: BN,
  three: BN,
  platformPda: web3.PublicKey
) => {
  await program.methods
    .updatePlatform(one, two, three)
    .accounts({
      admin: admin.publicKey,
      platformState: platformPda,
    })
    .signers([admin])
    .rpc();

  const platformDataFetch = await program.account.platformData.fetch(
    platformPda
  );
  return platformDataFetch;
};
