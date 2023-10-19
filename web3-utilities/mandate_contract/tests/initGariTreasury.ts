import { web3, Program } from "@coral-xyz/anchor";
import { MandateContract } from "../target/types/mandate_contract";

export const initGariTreasury = async (
  program: Program<MandateContract>,
  admin: web3.Keypair,
  treasuryGariAta: web3.PublicKey,
  gariTreasuryState: web3.PublicKey,
  payer: web3.Keypair
) => {
  await program.methods
    .initGariTreasury()
    .accounts({
      gariTreasuryState,
      admin: admin.publicKey,
      treasuryAccount: treasuryGariAta,
      payer: payer.publicKey,
      systemProgram: web3.SystemProgram.programId,
    })
    .signers([admin])
    .rpc();

  const gariTreasury = await program.account.gariTreasuryState.fetch(
    gariTreasuryState
  );
  return gariTreasury;
};
