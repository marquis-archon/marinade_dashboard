use crate::Command;

use cli_common::{
    instruction_helpers::InstructionHelpers, rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::RpcMarinade, transaction_builder::TransactionBuilder,
    transaction_helpers::TransactionBuilderHelpers, InputKeypair,
};
use log::info;

use cli_common::solana_sdk::native_token::sol_to_lamports;

use structopt::StructOpt;

use super::Common;

#[derive(Debug, StructOpt)]
pub struct Stake {
    #[structopt(
        short = "f",
        env = "FEE_PAYER",
        default_value = "~/.config/solana/id.json"
    )]
    fee_payer: InputKeypair,

    #[structopt(name = "amount")]
    amount: f64,
}

impl Command for Stake {
    fn process(self, _common: Common, marinade: RpcMarinade) -> anyhow::Result<()> {
        info!("Using fee payer {}", self.fee_payer);

        let mut builder = TransactionBuilder::limited(self.fee_payer.as_keypair());

        // TODO: separate fee payer from user wallet
        // find or create the associated (canonical) msol token account for the user
        let user_msol_account = builder.get_or_create_associated_token_account(
            marinade.client.clone(),
            &self.fee_payer.as_pubkey(),
            &marinade.state.msol_mint,
            "user mSOL",
        )?;

        builder.deposit(
            &marinade.state,
            self.fee_payer.as_keypair(), // TODO: choose different keypair from command line arg
            user_msol_account,
            sol_to_lamports(self.amount),
        );

        marinade
            .client
            .execute_transaction_sequence(builder.combined_sequence())?;

        Ok(())
    }
}
