import { web3, Program } from "@coral-xyz/anchor";
import { MandateContract } from "../target/types/mandate_contract";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";

export const revokeUserMandate = async (
  program: Program<MandateContract>,
  user: web3.Keypair,
  userMandateState: web3.PublicKey,
  userTokenAccount: web3.PublicKey,
  platformState: web3.PublicKey
) => {
  await program.methods
    .revokeUserMandate()
    .accounts({
      userMandateState,
      user: user.publicKey,
      tokenProgram: TOKEN_PROGRAM_ID,
      userTokenAccount,
      platformState,
      systemProgram: web3.SystemProgram.programId,
    })
    .signers([user])
    .rpc();

  const userMandate = await program.account.userMandateData.fetch(
    userMandateState
  );
  return userMandate;
};
