import { web3, Program, BN } from "@coral-xyz/anchor";
import { MandateContract } from "../target/types/mandate_contract";
import { TOKEN_PROGRAM_ID } from "@solana/spl-token";

export const initUserMandate = async (
  program: Program<MandateContract>,
  user: web3.Keypair,
  platformState: web3.PublicKey,
  userGariAta: web3.PublicKey,
  userMandateState: web3.PublicKey,
  mandateAmount: BN,
  validity: BN,
  maxTransactionAmount: BN,
  payer: web3.Keypair
) => {
  await program.methods
    .initUserMandate(mandateAmount, validity, maxTransactionAmount)
    .accounts({
      userMandateState,
      platformState,
      user: user.publicKey,
      userTokenAccount: userGariAta,
      payer: payer.publicKey,
      tokenProgram: TOKEN_PROGRAM_ID,
      systemProgram: web3.SystemProgram.programId,
    })
    .signers([user])
    .rpc();

  const userMandate = await program.account.userMandateData.fetch(
    userMandateState
  );
  return userMandate;
};
