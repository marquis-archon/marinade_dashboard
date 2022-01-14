use crate::Common;
use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::solana_sdk::program_pack::Pack;
use cli_common::solana_sdk::{native_token::sol_to_lamports, pubkey::Pubkey};
use cli_common::spl_token::state::{Account as Token, Mint};
use cli_common::transaction_helpers::TransactionBuilderHelpers;
use cli_common::{
    rpc_client_helpers::RpcClientHelpers, transaction_builder::TransactionBuilder, ExpandedPath,
};
use cli_common::{spl_token, InputPubkey};
use log::info;

use std::fs::File;
use std::io::Write;
use std::sync::Arc;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct TransferSplTokenOptions {
    #[structopt(short = "f", long = "from")]
    from: InputPubkey,

    #[structopt(short = "t", long = "to")]
    to: InputPubkey,

    #[structopt(short = "a", long = "auth", help = "source account authority")]
    auth: InputPubkey,

    #[structopt(
        short = "u",
        long = "sol-units",
        help = "amount in 1e9 units. Token must use 9 decimals"
    )]
    amount_sol: f64,

    #[structopt(short = "p", help = "propose transaction as binary file for multisig")]
    propose_output: ExpandedPath,
}

impl TransferSplTokenOptions {
    pub fn process(self, common: Common, client: Arc<RpcClient>) -> anyhow::Result<()> {
        // Print transaction to stdout in multisig format
        use ::borsh::BorshSerialize;
        use multisig::{TransactionAccount, TransactionInstruction};

        // check From account
        let from_account_info: Token;
        if let Some(from_account) = client.get_account_retrying(&self.from.as_pubkey())? {
            // here "owner" refers to owner-program
            if from_account.owner != spl_token::ID {
                panic!(
                    "Wrong FROM SPL token account {} owner {}",
                    self.from, from_account.owner
                );
            }
            from_account_info = Token::unpack_from_slice(&from_account.data)?;
            info!("Mint: {}", from_account_info.mint);
            // here "owner" refers to token-account auth
            if from_account_info.owner != self.auth.as_pubkey() {
                panic!(
                    "From Token-account.owner is {} but auth selected is {}",
                    from_account_info.owner, self.auth
                );
            }
        } else {
            panic!("can not read from_account")
        };

        // check To account
        let result = client.get_account_retrying(&self.to.as_pubkey())?;
        if result.is_none() {
            panic!("can not read to_account");
        }

        let to_account = result.unwrap();

        let destination: Pubkey;
        if to_account.owner == spl_token::ID {
            destination = self.to.as_pubkey();
        } else if to_account.owner == cli_common::solana_sdk::system_program::ID {
            // if _TO_ account is native, get/create the ATA
            let mut builder = TransactionBuilder::limited(common.fee_payer.as_keypair());
            let ata = builder.get_or_create_associated_token_account(
                &client,
                &self.to.as_pubkey(),
                &from_account_info.mint,
                "destination ATA",
            )?;
            // we might need to create the ATA
            if !builder.is_empty() {
                client.execute_transaction(builder.build_one())?;
            }
            info!("Using Associated Token address of {}: {}", &self.to, ata);
            destination = ata;
        } else {
            panic!(
                "Wrong TO SPL token account {} owner {}",
                self.to, to_account.owner
            );
        }

        let result = client.get_account_retrying(&destination)?;
        if result.is_none() {
            panic!("can not read destination account");
        }
        let to_account_info = Token::unpack_from_slice(&result.unwrap().data)?;

        //check same mint
        if from_account_info.mint != to_account_info.mint {
            panic!(
                "from and to mint do not match, from {} to {}",
                from_account_info.mint, to_account_info.mint
            );
        }

        if let Some(mint) = client.get_account_retrying(&from_account_info.mint)? {
            if mint.owner != spl_token::ID {
                panic!("mint {} Wrong owner {}", from_account_info.mint, mint.owner);
            }
            let mint_info = Mint::unpack_from_slice(&mint.data)?;
            if mint_info.decimals != 9 {
                panic!("mint must use 9 decimals")
            }
        } else {
            panic!("can not read accounts mint");
        }

        let instruction = spl_token::instruction::transfer(
            &spl_token::ID,
            &self.from.as_pubkey(),
            &destination,
            &self.auth.as_pubkey(),
            &[],
            sol_to_lamports(self.amount_sol),
        )?;
        info!(
            "instruction-data: {}",
            base64::encode(instruction.data.clone())
        );

        let transaction = TransactionInstruction {
            program_id: spl_token::ID,
            accounts: instruction
                .accounts
                .iter()
                .map(TransactionAccount::from)
                .collect(),
            data: instruction.data,
        };

        if self.propose_output.to_str().unwrap() != "data" {
            File::create(self.propose_output.as_path())?.write_all(&transaction.try_to_vec()?)?;
            info!("tx saved in {}", self.propose_output);
        }
        Ok(())
    }
}
