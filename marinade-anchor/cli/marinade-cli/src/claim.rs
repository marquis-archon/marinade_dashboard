use anyhow::Result;
use cli_common::{
    instruction_helpers::InstructionHelpers, rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::RpcMarinade, transaction_builder::TransactionBuilder, InputKeypair,
};
use log::info;

use cli_common::solana_sdk::pubkey::Pubkey;
use structopt::StructOpt;

use crate::Command;

use super::Common;

#[derive(Debug, StructOpt)]
pub struct Claim {
    #[structopt(
        short = "f",
        env = "FEE_PAYER",
        default_value = "~/.config/solana/id.json"
    )]
    fee_payer: InputKeypair,

    #[structopt(name = "ticket")]
    ticket_account: Pubkey,
}

impl Command for Claim {
    fn process(self, _common: Common, marinade: RpcMarinade) -> Result<()> {
        //
        info!("Using fee payer {}", self.fee_payer);

        //let rent: Rent = bincode::deserialize(&marinade.client.get_account_data(&rent::id())?)?;

        let mut builder = TransactionBuilder::limited(self.fee_payer.as_keypair());

        //let ticket: TicketAccountData = AccountDeserialize::try_deserialize( marinade.client.get_account_data(ticket_account).as_slice);

        // Create a Claim instruction.
        builder.claim(
            &marinade.state,
            self.ticket_account,
            self.fee_payer.as_pubkey(), //ticket beneficiary
        );

        marinade
            .client
            .execute_transaction_sequence(builder.combined_sequence())?;

        Ok(())
    }
}
