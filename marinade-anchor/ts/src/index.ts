import { exec } from 'child_process';
import assert from "assert";
import { Account, AccountInfo, Connection, LAMPORTS_PER_SOL, PublicKey, StakeActivationData } from "@solana/web3.js";
import fs from 'mz/fs';
import { deserialize, Schema } from 'borsh';
import * as marinade from './marinade_finance_schema';
import BN from 'bn.js';
import { inspect } from 'util';

import * as anchor from '@project-serum/anchor';
import { TOKEN_PROGRAM_ID } from '@solana/spl-token';

export async function execShellCommand(cmd: string): Promise<String> {
    return new Promise<String>((resolve, reject) => {
        exec(cmd, (error, stdout, stderr) => {
            if (error) {
                console.warn(error);
            }
            resolve(stdout ? stdout : stderr);
        });
    });
}

async function readAccount(fileName: string): Promise<Account> {
    return new Account(JSON.parse(await fs.readFile(fileName, 'utf-8')))
}


//-------- GET & DECODE STATE ACCOUNT ------------//
async function showStateAccount(stateAccountPubKey: PublicKey): Promise<marinade.State> {

    const connection = anchor.getProvider().connection
    const stateAccount = await connection.getAccountInfo(stateAccountPubKey);
    //console.log("stateAccount:", inspect(stateAccount, false, 99, true));
    if (!stateAccount) throw Error("stateAccount is null/undefined");

    const state: marinade.State = deserialize(marinade.MARINADE_BORSH_SCHEMA, marinade.State, stateAccount!.data.slice(8));

    //console.log("state:", inspect(state, false, 1, true));
    console.log("state.epoch_stake_orders ", state.epoch_stake_orders.toNumber() / anchor.web3.LAMPORTS_PER_SOL)
    console.log("state.epoch_unstake_orders ", state.epoch_unstake_orders.toNumber() / anchor.web3.LAMPORTS_PER_SOL)

    // const staked = state.validator_system.total_balance.add(state.epoch_stake_orders).sub(state.epoch_unstake_orders);
    // const st_sol_supply = Number((await connection.getTokenSupply(state.st_mint.value)).value.amount);
    // const st_sol_price = st_sol_supply > 0 ? (Number(staked) / st_sol_supply) : 1;
    console.log(`mSOL price: ${state.st_sol_price.toNumber() / 0x1_0000_0000}`);

    return state;
}

export async function findPDA(baseAddressSeed: PublicKey, seed: string, programId: PublicKey): Promise<PublicKey> {
    const SEED_AS_BYTES = new TextEncoder().encode(seed)
    let [address, bump] = await PublicKey.findProgramAddress([baseAddressSeed.toBytes(), SEED_AS_BYTES], programId)
    return address
}

/**
 * find the associated token address (canonical)
 * given the main user's address (wallet) and the Token Mint
 * @param walletAddress 
 * @param tokenMintAddress 
 * @returns AssocTokenAccount PublicKey
 */
async function findAssociatedTokenAddress(
    walletAddress: PublicKey,
    tokenMintAddress: PublicKey
): Promise<PublicKey> {

    const SPL_ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID = new PublicKey('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL');

    let [address, bump] = await PublicKey.findProgramAddress(
        [
            walletAddress.toBuffer(),
            TOKEN_PROGRAM_ID.toBuffer(),
            tokenMintAddress.toBuffer(),
        ],
        SPL_ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID
    );

    return address;
}


export async function run(): Promise<void> {

    //-------- GET SOLANA CONFIG ------------//

    // const solana_config = (await execShellCommand('solana config get')).split('\n');
    // let url = null;
    // let payerAccount = null;
    // for (let line of solana_config) {
    //     line = line.trimRight();
    //     let m = line.match(/^RPC URL: (.*)$/i);
    //     if (m) {
    //         url = m[1];
    //     }

    //     m = line.match(/^Keypair Path: (.*)$/);
    //     if (m) {
    //         payerAccount = m[1];
    //     }
    // }
    // assert.ok(url, "Can't parse solana config RPC URL");
    // assert.ok(payerAccount, "Can't parse solana config account");

    // console.log(url!)
    // const connection = new Connection(url!, 'singleGossip');

    // const payer = await readAccount(payerAccount!);

    //-----------------------------------------
    anchor.setProvider(anchor.Provider.local());

    const wallet = anchor.getProvider().wallet;

    // Address of the deployed program.
    const programId = new anchor.web3.PublicKey('5HeJRkxvJdYCnZuGnKrUuekFSoH1HKcrJVNPS1zZUXCt');
    // Address of main state (instance)
    const stateAccountPubKey = new anchor.web3.PublicKey("Aw8kKMxGRsBQxpPcqP45sYbTtFtYte9x42RiUWrdLUCH");

    //-- show state before
    let state = await showStateAccount(stateAccountPubKey)
    //-----------------------------------------

    //-------- GET IDL ------------//

    console.log(process.cwd())
    var idl_text = fs.readFileSync('../target/idl/marinade_program.json').toString();
    //console.log(typeof idl_text)
    //idl_text = idl_text.replace(/"u16"/g,'"u32"')
    const idl = JSON.parse(idl_text);

    //-------- CALL STAKE (deposit) ----------//

    // Generate the program client from IDL, on the fly, in memory
    const program = new anchor.Program(idl, programId);

    const SOL = anchor.web3.LAMPORTS_PER_SOL;
    var deposit_lamports = 1 * SOL;

    console.log("-------------------------------")
    console.log(`CALLING program.rpc.deposit lamports=${deposit_lamports} via ANCHOR IDL...`)
    //console.log("liqPoolStSolLeg:", statePre.liq_pool.st_sol_account.value.toBase58()); //75XZmMDSzkH67rbw3d2tNFytAao4deUGxtBTUw4q8NU6

    //compute some needed PDA's
    const RESERVE_PDA_SEED = "reserve"
    const reservePDA = await findPDA(stateAccountPubKey, RESERVE_PDA_SEED, programId)

    const LIQ_POOL_SOL_ACCOUNT_SEED = "liq_sol"
    const liqPoolSolAccountPDA = await findPDA(stateAccountPubKey, LIQ_POOL_SOL_ACCOUNT_SEED, programId)

    const LIQ_POOL_ST_SOL_AUTH_SEED = "liq_st_sol_authority"
    const liqPoolStSolAuth = await findPDA(stateAccountPubKey, LIQ_POOL_ST_SOL_AUTH_SEED, programId)

    const LIQ_POOL_ST_SOL_MINT_AUTH_SEED = "st_mint"
    const liqPoolStSolMintAuth = await findPDA(stateAccountPubKey, LIQ_POOL_ST_SOL_MINT_AUTH_SEED, programId)

    //compute user's associated (default, canonical) token account
    const userAssociatedTokenAccount = await findAssociatedTokenAddress(wallet.publicKey, state.st_mint.value)

    //Anchor will parse the accounts we pass as parameters and compose the tx
    let result = await program.rpc.deposit(new anchor.BN(deposit_lamports),
        {
            accounts: {
                state: stateAccountPubKey,
                stSolMint: state.st_mint.value,

                liqPoolSolAccountPda: liqPoolSolAccountPDA,

                liqPoolStSolLeg: state.liq_pool.st_sol_account.value,
                liqPoolStSolLegAuthority: liqPoolStSolAuth,

                reservePda: reservePDA,

                transferFrom: wallet.publicKey,

                mintTo: userAssociatedTokenAccount,

                stSolMintAuthority: liqPoolStSolMintAuth,

                systemProgram: anchor.web3.SystemProgram.programId,
                tokenProgram: TOKEN_PROGRAM_ID,
            }
        }
    );
    console.log("tx:", result);
    console.log("waiting for confirmation...")
    // wait for the TX to confirm (if we don't wait we will read the account again without the changes applied)
    const status = await anchor.getProvider().connection.confirmTransaction(result)
    if (status.value.err) {
        throw Error(`Transaction ${result} failed (${JSON.stringify(status)})`);
    }
    console.log("tx status:", status);
    console.log("-------------------------------")

    //-- show state after
    // Note: always wait for the TX to confirm (if we don't wait we will read the account again without the changes applied)
    let statePost = await showStateAccount(stateAccountPubKey)

}