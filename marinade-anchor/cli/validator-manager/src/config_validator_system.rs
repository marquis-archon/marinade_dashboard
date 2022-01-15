use crate::Common;

use anyhow::bail;
use cli_common::{
    instruction_helpers::InstructionHelpers, rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::RpcMarinade, transaction_builder::TransactionBuilder,
};
use log::{error, info};

use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::solana_sdk::{signature::Signer, system_program};

use std::sync::Arc;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct ConfigValidatorsOptions {
    #[structopt(long = "extra-runs")]
    extra_runs: u32,
}

impl ConfigValidatorsOptions {
    pub fn process(self, common: Common, client: Arc<RpcClient>) -> anyhow::Result<()> {
        //
        //prepare txn builder
        let mut builder = TransactionBuilder::limited(common.fee_payer.as_keypair());

        let marinade = RpcMarinade::new(client, &common.instance.as_pubkey())?;

        let rent_payer = common.fee_payer.as_keypair();

        if let Some(account) = marinade.client.get_account_retrying(&rent_payer.pubkey())? {
            if account.owner != system_program::ID {
                error!(
                    "Rent payer {} must be a system account",
                    rent_payer.pubkey()
                );
                bail!(
                    "Rent payer {} must be a system account",
                    rent_payer.pubkey()
                );
            }
        }

        let validator_manager_authority =
            if let Some(validator_manager_authority) = common.validator_manager_authority {
                info!(
                    "Using validator manager authority {}",
                    validator_manager_authority
                );
                validator_manager_authority.as_keypair()
            } else {
                info!("Using fee payer as validator manager authority");
                common.fee_payer.as_keypair()
            };

        builder.config_validator_system(
            &marinade.state,
            validator_manager_authority,
            self.extra_runs,
        )?;

        // send the tx
        info!("sending transactions");
        marinade
            .client
            .process_transaction_sequence(common.simulate, builder.combined_sequence())?;

        Ok(())
    }
}
