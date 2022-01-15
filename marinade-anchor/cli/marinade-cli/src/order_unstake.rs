use anyhow::{bail, Result};
use cli_common::{
    instruction_helpers::InstructionHelpers, marinade_finance::ticket_account::TicketAccountData,
    rpc_client_helpers::RpcClientHelpers, rpc_marinade::RpcMarinade,
    transaction_builder::TransactionBuilder, InputKeypair,
};
use log::{error, info};

use cli_common::spl_associated_token_account::get_associated_token_address;

use std::sync::Arc;

use cli_common::solana_sdk::{
    native_token::sol_to_lamports,
    rent::Rent,
    signature::{Keypair, Signer},
    sysvar::rent,
};
use structopt::StructOpt;

use crate::Command;

use super::Common;

#[derive(Debug, StructOpt)]
pub struct OrderUnstake {
    #[structopt(
        short = "f",
        env = "FEE_PAYER",
        default_value = "~/.config/solana/id.json"
    )]
    fee_payer: InputKeypair,

    #[structopt(name = "msol_amount")]
    msol_amount: f64,
}

impl Command for OrderUnstake {
    fn process(self, _common: Common, marinade: RpcMarinade) -> Result<()> {
        //
        info!("Using fee payer {}", self.fee_payer);

        let rent: Rent = bincode::deserialize(&marinade.client.get_account_data(&rent::id())?)?;

        let mut builder = TransactionBuilder::limited(self.fee_payer.as_keypair());

        // find the associated (canonical) msol token account for the user
        let user_msol_account =
            get_associated_token_address(&self.fee_payer.as_pubkey(), &marinade.state.msol_mint);
        // TODO: check balance
        if marinade
            .client
            .get_account_retrying(&user_msol_account)?
            .is_none()
        {
            error!("Can not find user mSOL account {}", user_msol_account);
            bail!("Can not find user mSOL account {}", user_msol_account);
        }

        // Create a empty ticket account (transfer rent-exempt lamports)
        const TICKET_ACCOUNT_SPACE: usize = 8 + std::mem::size_of::<TicketAccountData>();
        let ticket_account = Arc::new(Keypair::new());
        let ticket_address = ticket_account.pubkey();
        builder
            .create_account(
                ticket_account,
                TICKET_ACCOUNT_SPACE,
                &cli_common::marinade_finance::ID,
                &rent,
                "ticket-account",
            )
            .unwrap();

        builder.order_unstake(
            &marinade.state,
            user_msol_account,
            self.fee_payer.as_keypair(),
            sol_to_lamports(self.msol_amount),
            ticket_address,
            // self.fee_payer.as_pubkey(),
        );

        marinade
            .client
            .execute_transaction_sequence(builder.combined_sequence())?;

        info!("Unstake order created, you'll have to wait two epochs + 4 hours to claim your SOL");

        Ok(())
    }
}
