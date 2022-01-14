use crate::{Command, Common};

use anyhow::{bail, Result};
use cli_common::{
    instruction_helpers::InstructionHelpers, rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::RpcMarinade, transaction_builder::TransactionBuilder,
    transaction_helpers::TransactionBuilderHelpers, InputKeypair,
};
use log::{error, info};

use cli_common::solana_sdk::native_token::sol_to_lamports;
use cli_common::spl_associated_token_account::get_associated_token_address;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct AddLiquidity {
    #[structopt(
        short = "f",
        env = "FEE_PAYER",
        default_value = "~/.config/solana/id.json"
    )]
    fee_payer: InputKeypair,

    #[structopt(name = "amount")]
    amount: f64,
}

impl Command for AddLiquidity {
    fn process(self, _common: Common, marinade: RpcMarinade) -> Result<()> {
        info!("Using fee payer {}", self.fee_payer);

        let mut builder = TransactionBuilder::limited(self.fee_payer.as_keypair());

        // start preparing instructions
        // find or create the associated (canonical) smart-lp token account for the user
        let user_smart_lp_account = builder.get_or_create_associated_token_account(
            marinade.client.clone(),
            &self.fee_payer.as_pubkey(),
            &marinade.state.liq_pool.lp_mint,
            "smart-lp",
        )?;

        builder.add_liquidity(
            &marinade.state,
            self.fee_payer.as_keypair(),
            user_smart_lp_account,
            sol_to_lamports(self.amount),
        );

        marinade
            .client
            .execute_transaction_sequence(builder.combined_sequence())?;

        Ok(())
    }
}

//------------------------------------

#[derive(Debug, StructOpt)]
pub struct RemoveLiquidity {
    #[structopt(
        short = "f",
        env = "FEE_PAYER",
        default_value = "~/.config/solana/id.json"
    )]
    fee_payer: InputKeypair,

    #[structopt(name = "LP-token-amount")]
    amount: f64,
}

impl Command for RemoveLiquidity {
    fn process(self, _common: Common, marinade: RpcMarinade) -> Result<()> {
        info!("Using fee payer {}", self.fee_payer);

        let mut builder = TransactionBuilder::limited(self.fee_payer.as_keypair());

        //start preparing instructions
        //find or create the associated (canonical) smart-lp token account for the user
        let user_smart_lp_account = get_associated_token_address(
            &self.fee_payer.as_pubkey(),
            &marinade.state.liq_pool.lp_mint,
        );

        // TODO: check balance
        if marinade
            .client
            .get_account_retrying(&user_smart_lp_account)?
            .is_none()
        {
            error!("Can not find user lp account {}", user_smart_lp_account);
            bail!("Can not find user lp account {}", user_smart_lp_account);
        }

        // find or create the associated (canonical) msol token account for the user
        let user_msol_account = builder.get_or_create_associated_token_account(
            marinade.client.clone(),
            &self.fee_payer.as_pubkey(),
            &marinade.state.msol_mint,
            "user mSOL",
        )?;

        builder.remove_liquidity(
            &marinade.state,
            user_smart_lp_account,
            self.fee_payer.as_keypair(),
            self.fee_payer.as_pubkey(),
            user_msol_account,
            sol_to_lamports(self.amount),
        );

        marinade
            .client
            .execute_transaction_sequence(builder.combined_sequence())?;

        Ok(())
    }
}
