import { web3, Program } from "@coral-xyz/anchor";
import { MandateContract } from "../target/types/mandate_contract";

export const removeGariTreasury = async (
  program: Program<MandateContract>,
  admin: web3.Keypair,
  treasuryGariAta: web3.PublicKey,
  gariTreasuryState: web3.PublicKey,
  payer: web3.Keypair
) => {
  await program.methods
    .removeGariTreasury()
    .accounts({
      gariTreasuryState,
      admin: admin.publicKey,
      treasuryAccount: treasuryGariAta,
      payer: payer.publicKey,
    })
    .signers([admin])
    .rpc();
};
