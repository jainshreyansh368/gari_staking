import {
  Program,
  web3,
  utils,
  BN,
  setProvider,
  AnchorProvider,
  workspace,
  Wallet,
} from "@coral-xyz/anchor";
import { MandateContract } from "../target/types/mandate_contract";
import { initPlatform } from "./initPlatform";
import { updatePlatform } from "./updatePlatform";
import { initGariTreasury } from "./initGariTreasury";
import { initUserMandate } from "./initUserMandate";
import { updateUserMandate } from "./updateUserMandate";
import { revokeUserMandate } from "./revokeUserMandate";
import { transferGariToTreasury } from "./transferGariToTreasury";
import { removeGariTreasury } from "./removeGariTreasury";
import {
  getAssociatedTokenAddress,
  getAccount,
  createAssociatedTokenAccountInstruction,
  createTransferCheckedInstruction,
  MINT_SIZE,
  getMinimumBalanceForRentExemptMint,
  TOKEN_PROGRAM_ID,
  createInitializeMintInstruction,
  createMintToCheckedInstruction,
} from "@solana/spl-token";
import { expect } from "chai";
import { Connection } from "@solana/web3.js";

describe("mandate_contract", () => {
  const admin = web3.Keypair.fromSecretKey(
    Uint8Array.from([
      27, 43, 88, 139, 14, 84, 29, 51, 131, 43, 34, 58, 199, 197, 120, 252, 127,
      137, 211, 183, 126, 106, 134, 56, 251, 5, 200, 144, 224, 57, 231, 123,
      233, 226, 120, 244, 95, 85, 8, 191, 229, 197, 98, 57, 158, 232, 138, 122,
      171, 136, 162, 122, 86, 44, 201, 212, 231, 252, 1, 155, 129, 46, 195, 164,
    ])
  );

  const mint = web3.Keypair.fromSecretKey(
    Uint8Array.from([
      158, 224, 75, 70, 237, 128, 106, 87, 16, 22, 194, 68, 216, 193, 234, 160,
      20, 124, 15, 238, 100, 91, 76, 43, 241, 74, 105, 230, 99, 164, 107, 43,
      13, 177, 153, 139, 182, 20, 66, 144, 97, 76, 156, 20, 221, 194, 170, 51,
      144, 107, 7, 208, 174, 111, 175, 171, 115, 71, 150, 195, 81, 224, 166, 4,
    ])
  );

  const user = web3.Keypair.fromSecretKey(
    Uint8Array.from([
      178, 253, 40, 1, 37, 116, 76, 138, 118, 93, 28, 20, 57, 68, 117, 213, 204,
      199, 2, 22, 93, 219, 25, 40, 155, 80, 72, 16, 77, 211, 65, 173, 11, 55,
      149, 216, 133, 87, 175, 7, 107, 39, 43, 253, 62, 248, 2, 199, 187, 172,
      213, 223, 127, 192, 131, 187, 20, 189, 170, 197, 19, 93, 135, 49,
    ])
  );

  const payer = web3.Keypair.fromSecretKey(
    Uint8Array.from([
      232, 63, 188, 241, 66, 97, 180, 5, 22, 140, 15, 71, 240, 243, 134, 111,
      139, 23, 129, 37, 19, 30, 103, 42, 253, 76, 161, 3, 23, 167, 7, 245, 131,
      24, 213, 254, 105, 238, 47, 192, 164, 62, 255, 11, 252, 33, 94, 4, 222,
      92, 47, 129, 52, 80, 179, 203, 190, 75, 241, 163, 70, 255, 29, 51,
    ])
  );

  let provider = new AnchorProvider(
    new Connection("http://localhost:8899", "processed"),
    new Wallet(payer),
    {
      commitment: "processed",
      skipPreflight: false,
      preflightCommitment: "processed",
    }
  );

  setProvider(provider);

  const program = workspace.MandateContract as Program<MandateContract>;

  const [platformPda] = web3.PublicKey.findProgramAddressSync(
    [Buffer.from(utils.bytes.utf8.encode("mandate_data"))],
    program.programId
  );

  before("Airdrop wallet(s) and setup token accounts.", async () => {
    const signature = await program.provider.connection.requestAirdrop(
      payer.publicKey,
      3000000000
    );
    const latestBlockhash =
      await program.provider.connection.getLatestBlockhash();
    await program.provider.connection.confirmTransaction(
      {
        signature,
        ...latestBlockhash,
      },
      "confirmed"
    );

    let treasuryGariAta = await getAssociatedTokenAddress(
      mint.publicKey,
      program.provider.publicKey
    );

    const createMintTx = new web3.Transaction().add(
      web3.SystemProgram.createAccount({
        fromPubkey: program.provider.publicKey,
        newAccountPubkey: mint.publicKey,
        space: MINT_SIZE,
        lamports: await getMinimumBalanceForRentExemptMint(
          program.provider.connection
        ),
        programId: TOKEN_PROGRAM_ID,
      }),

      createInitializeMintInstruction(
        mint.publicKey,
        8,
        program.provider.publicKey,
        program.provider.publicKey
      ),
      createAssociatedTokenAccountInstruction(
        program.provider.publicKey,
        treasuryGariAta,
        program.provider.publicKey,
        mint.publicKey
      ),
      createMintToCheckedInstruction(
        mint.publicKey,
        treasuryGariAta,
        program.provider.publicKey,
        100000000 * 100000000,
        8
      )
    );

    await program.provider.sendAndConfirm(createMintTx, [mint]);

    let userGariAta = await getAssociatedTokenAddress(
      mint.publicKey,
      user.publicKey
    );

    const createUserATATx = new web3.Transaction().add(
      createAssociatedTokenAccountInstruction(
        program.provider.publicKey,
        userGariAta,
        user.publicKey,
        mint.publicKey
      ),
      createTransferCheckedInstruction(
        treasuryGariAta,
        mint.publicKey,
        userGariAta,
        program.provider.publicKey,
        100000000 * 10000,
        8
      )
    );

    await program.provider.sendAndConfirm(createUserATATx);
  });

  it("Admin Should be able to init platform.", async () => {
    const platform = await initPlatform(program, admin, platformPda, payer);

    expect(platform.isInitialized).equal(true);
    expect(platform.admin.toString()).equal(admin.publicKey.toString());
    expect(platform.minMandateAmount.toNumber()).equal(12);
    expect(platform.minValidity.toNumber()).equal(12);
    expect(platform.maxTxAmount.toNumber()).equal(12);
  });

  it("Admin Should be able to update platform.", async () => {
    const one = new BN(13);
    const two = new BN(13);
    const three = new BN(13);

    const platform = await updatePlatform(
      program,
      admin,
      one,
      two,
      three,
      platformPda
    );
    expect(platform.isInitialized).equal(true);
    expect(platform.admin.toString()).equal(admin.publicKey.toString());
    expect(platform.minMandateAmount.toNumber()).equal(13);
    expect(platform.minValidity.toNumber()).equal(13);
    expect(platform.maxTxAmount.toNumber()).equal(13);
  });

  it("Admin Should be able to init gari treasury.", async () => {
    let treasuryGariAta = await getAssociatedTokenAddress(
      mint.publicKey,
      program.provider.publicKey
    );

    const [gariTreasuryState] = web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from(utils.bytes.utf8.encode("gari_treasury")),
        treasuryGariAta.toBuffer(),
      ],
      program.programId
    );

    const gariTreasury = await initGariTreasury(
      program,
      admin,
      treasuryGariAta,
      gariTreasuryState,
      payer
    );
    expect(gariTreasury.isInitialized).equal(true);
    expect(gariTreasury.treasuryAccount.toString()).equal(
      treasuryGariAta.toString()
    );
  });

  it("User Should be able to init user mandate.", async () => {
    const [userMandateState] = web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from(utils.bytes.utf8.encode("mandate_data")),
        user.publicKey.toBuffer(),
      ],
      program.programId
    );

    let userGariAta = await getAssociatedTokenAddress(
      mint.publicKey,
      user.publicKey
    );

    const mandateAmount = new BN(13);
    const validity = new BN(4680083491);
    const maxTransactionAmount = new BN(10);

    let userGariAtaAccount = await getAccount(
      program.provider.connection,
      userGariAta
    );

    expect(userGariAtaAccount.delegate).equal(null);
    expect(userGariAtaAccount.delegatedAmount.toString()).equal("0");

    const userMandate = await initUserMandate(
      program,
      user,
      platformPda,
      userGariAta,
      userMandateState,
      mandateAmount,
      validity,
      maxTransactionAmount,
      payer
    );

    userGariAtaAccount = await getAccount(
      program.provider.connection,
      userGariAta
    );

    expect(userGariAtaAccount.delegate.toString()).equal(
      userMandateState.toString()
    );
    expect(userGariAtaAccount.delegatedAmount.toString()).equal("13");

    expect(userMandate.isInitialized).equal(true);
    expect(userMandate.user.toString()).equal(user.publicKey.toString());
    expect(userMandate.userTokenAccount.toString()).equal(
      userGariAta.toString()
    );
    expect(userMandate.approvedAmount.toNumber()).equal(
      mandateAmount.toNumber()
    );
    expect(userMandate.amountTransfered.toNumber()).equal(0);
    expect(userMandate.amountPerTransaction.toNumber()).equal(
      maxTransactionAmount.toNumber()
    );
    // validity timestamp cannto be determined for test, because of timestamp might change on the chain
    // expect(userMandate.mandateValidity.toNumber()).equal(validity.toNumber());
    expect(userMandate.revoked).equal(false);
  });

  it("User Should be able to update user mandate.", async () => {
    const mandateAmount = new BN(12);
    const validity = new BN(9680082867);
    const maxTransactionAmount = new BN(9);

    const [userMandateState] = web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from(utils.bytes.utf8.encode("mandate_data")),
        user.publicKey.toBuffer(),
      ],
      program.programId
    );

    let userGariAta = await getAssociatedTokenAddress(
      mint.publicKey,
      user.publicKey
    );
    const userMandate = await updateUserMandate(
      program,
      user,
      platformPda,
      userMandateState,
      userGariAta,
      mandateAmount,
      validity,
      maxTransactionAmount
    );

    let userGariAtaAccount = await getAccount(
      program.provider.connection,
      userGariAta
    );
    expect(userGariAtaAccount.delegate.toString()).equal(
      userMandateState.toString()
    );
    expect(userGariAtaAccount.delegatedAmount.toString()).equal("12");

    expect(userMandate.isInitialized).equal(true);
    expect(userMandate.user.toString()).equal(user.publicKey.toString());
    expect(userMandate.userTokenAccount.toString()).equal(
      userGariAta.toString()
    );
    expect(userMandate.approvedAmount.toNumber()).equal(
      13 + mandateAmount.toNumber()
    );
    expect(userMandate.amountTransfered.toNumber()).equal(0);
    expect(userMandate.amountPerTransaction.toNumber()).equal(
      maxTransactionAmount.toNumber()
    );
    // validity timestamp cannto be determined for test, because of timestamp might change on the chain
    // expect(userMandate.mandateValidity.toNumber()).equal(validity.toNumber());
    expect(userMandate.revoked).equal(false);
  });

  it("User Should be able to transfer gari to treasury.", async () => {
    const amount = new BN(2);
    const [userMandateState, bump] = web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from(utils.bytes.utf8.encode("mandate_data")),
        user.publicKey.toBuffer(),
      ],
      program.programId
    );

    let treasuryGariAta = await getAssociatedTokenAddress(
      mint.publicKey,
      program.provider.publicKey
    );

    let userGariAta = await getAssociatedTokenAddress(
      mint.publicKey,
      user.publicKey
    );

    const [gariTreasuryState] = web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from(utils.bytes.utf8.encode("gari_treasury")),
        treasuryGariAta.toBuffer(),
      ],
      program.programId
    );

    let treasuryGariAtaAccount = await getAccount(
      program.provider.connection,
      treasuryGariAta
    );
    let userGariAtaAccount = await getAccount(
      program.provider.connection,
      userGariAta
    );

    expect(treasuryGariAtaAccount.amount.toString()).equal("9999000000000000");

    expect(userGariAtaAccount.delegate.toString()).equal(
      userMandateState.toString()
    );
    expect(userGariAtaAccount.amount.toString()).equal("1000000000000");
    expect(userGariAtaAccount.delegatedAmount.toString()).equal("12");

    const userMandate = await transferGariToTreasury(
      program,
      user,
      userMandateState,
      treasuryGariAta,
      userGariAta,
      gariTreasuryState,
      amount,
      bump,
      platformPda
    );

    treasuryGariAtaAccount = await getAccount(
      program.provider.connection,
      treasuryGariAta
    );
    userGariAtaAccount = await getAccount(
      program.provider.connection,
      userGariAta
    );

    expect(treasuryGariAtaAccount.amount.toString()).equal("9999000000000002");

    expect(userGariAtaAccount.delegate.toString()).equal(
      userMandateState.toString()
    );
    expect(userGariAtaAccount.amount.toString()).equal("999999999998");
    expect(userGariAtaAccount.delegatedAmount.toString()).equal("10");

    expect(userMandate.isInitialized).equal(true);
    expect(userMandate.user.toString()).equal(user.publicKey.toString());
    expect(userMandate.userTokenAccount.toString()).equal(
      userGariAta.toString()
    );
    expect(userMandate.approvedAmount.toNumber()).equal(25);
    expect(userMandate.amountTransfered.toNumber()).equal(2);
    expect(userMandate.amountPerTransaction.toNumber()).equal(9);
    // validity timestamp cannto be determined for test, because of timestamp might change on the chain
    // expect(userMandate.mandateValidity.toNumber()).equal(validity.toNumber());
    expect(userMandate.revoked).equal(false);
  });

  it("User Should be able to revoke user mandate.", async () => {
    const [userMandateState] = web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from(utils.bytes.utf8.encode("mandate_data")),
        user.publicKey.toBuffer(),
      ],
      program.programId
    );

    let userGariAta = await getAssociatedTokenAddress(
      mint.publicKey,
      user.publicKey
    );

    const userMandate = await revokeUserMandate(
      program,
      user,
      userMandateState,
      userGariAta,
      platformPda
    );
    expect(userMandate.revoked).equal(true);

    let userGariAtaAccount = await getAccount(
      program.provider.connection,
      userGariAta
    );

    expect(userGariAtaAccount.delegate).equal(null);
    expect(userGariAtaAccount.amount.toString()).equal("999999999998");
    expect(userGariAtaAccount.delegatedAmount.toString()).equal("0");
  });

  it("Admin Should be able to remove gari treasury.", async () => {
    let treasuryGariAta = await getAssociatedTokenAddress(
      mint.publicKey,
      program.provider.publicKey
    );

    const [gariTreasuryState] = web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from(utils.bytes.utf8.encode("gari_treasury")),
        treasuryGariAta.toBuffer(),
      ],
      program.programId
    );

    let gariTreasury = await program.account.gariTreasuryState.fetch(
      gariTreasuryState
    );

    expect(gariTreasury.isInitialized).equal(true);
    expect(gariTreasury.treasuryAccount.toString()).equal(
      treasuryGariAta.toString()
    );

    await removeGariTreasury(
      program,
      admin,
      treasuryGariAta,
      gariTreasuryState,
      payer
    );

    try {
      await program.account.gariTreasuryState.fetch(gariTreasuryState);
      throw "Test: The fetch() call should return error.";
    } catch (err) {
      expect(err.toString()).equal(
        `Error: Account does not exist or has no data ${gariTreasuryState.toString()}`.toString()
      );
    }
  });
});
