import { web3, Program, BN } from "@coral-xyz/anchor";
import { MandateContract } from "../target/types/mandate_contract";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";

export const transferGariToTreasury = async (
  program: Program<MandateContract>,
  user: web3.Keypair,
  userMandateState: web3.PublicKey,
  treasuryGariAta: web3.PublicKey,
  userGariAta: web3.PublicKey,
  gariTreasuryState: web3.PublicKey,
  amount: BN,
  userMandateStateBump: any,
  platformState: web3.PublicKey
) => {
  await program.methods
    .transferToGariTreasury(amount, userMandateStateBump)
    .accounts({
      userMandateState,
      user: user.publicKey,
      gariTreasuryState,
      treasuryAccount: treasuryGariAta,
      userTokenAccount: userGariAta,
      tokenProgram: TOKEN_PROGRAM_ID,
      platformState,
      systemProgram: web3.SystemProgram.programId,
    })
    .signers([])
    .rpc();
  const userMandate = await program.account.userMandateData.fetch(
    userMandateState
  );
  return userMandate;
};
