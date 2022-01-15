use anyhow::{bail, Result};
use cli_common::{
    instruction_helpers::InstructionHelpers, rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::RpcMarinade, transaction_builder::TransactionBuilder, InputKeypair,
};
use log::{error, info};

use cli_common::spl_associated_token_account::get_associated_token_address;

use cli_common::solana_sdk::native_token::sol_to_lamports;
use structopt::StructOpt;

use crate::Command;

use super::Common;

#[derive(Debug, StructOpt)]
pub struct LiquidUnstake {
    #[structopt(
        short = "f",
        env = "FEE_PAYER",
        default_value = "~/.config/solana/id.json"
    )]
    fee_payer: InputKeypair,

    #[structopt(name = "msol_amount")]
    msol_amount: f64,
}

impl Command for LiquidUnstake {
    fn process(self, _common: Common, marinade: RpcMarinade) -> Result<()> {
        info!("Using fee payer {}", self.fee_payer);

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

        builder.liquid_unstake(
            &marinade.state,
            user_msol_account,
            self.fee_payer.as_keypair(),
            self.fee_payer.as_pubkey(),
            sol_to_lamports(self.msol_amount),
        );

        marinade
            .client
            .execute_transaction_sequence(builder.combined_sequence())?;

        Ok(())
    }
}
