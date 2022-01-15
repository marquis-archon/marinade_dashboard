use crate::Common;

use anyhow::anyhow;
use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::solana_sdk::pubkey::Pubkey;
use cli_common::{
    instruction_helpers::InstructionHelpers, rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::RpcMarinade, transaction_builder::TransactionBuilder,
};
use log::{error, info};

use std::sync::Arc;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct EmergencyUnstakeOptions {
    validator_vote: Pubkey,

    #[structopt(short = "r", long = "remove", help = "also remove the validator")]
    remove: bool,
}

impl EmergencyUnstakeOptions {
    pub fn process(self, common: Common, client: Arc<RpcClient>) -> anyhow::Result<()> {
        info!("Emergency unstake from validator {}", self.validator_vote);

        let mut builder = TransactionBuilder::limited(common.fee_payer.as_keypair());

        let marinade = RpcMarinade::new(client, &common.instance.as_pubkey())?;

        let validator_manager_authority =
            if let Some(validator_manager_authority) = &common.validator_manager_authority {
                info!(
                    "Using validator manager authority {}",
                    validator_manager_authority
                );
                validator_manager_authority.as_keypair()
            } else {
                info!("Using fee payer as validator manager authority");
                common.fee_payer.as_keypair()
            };
        let (validator_list, _) = marinade.validator_list()?;
        let validator_index = validator_list
            .iter()
            .position(|validator| validator.validator_account == self.validator_vote)
            .ok_or_else(|| {
                error!("Unknown validator {}", self.validator_vote);
                anyhow!("Unknown validator {}", self.validator_vote)
            })?;

        // if we want to zero the score before unstaking (Unstake all)
        builder.set_validator_score(
            &marinade.state,
            validator_manager_authority.clone(),
            validator_index as u32,
            self.validator_vote,
            0,
        )?;
        // Flush it asap.
        marinade
            .client
            .process_transaction_sequence(common.simulate, builder.combined_sequence())?;
        info!("validator score set to zero");

        // get accounts related to the validator, build transaction
        info!("looking for accounts delegated to {}", self.validator_vote);
        let (stakes_info, _) = marinade.stakes_info_reversed()?;
        for stake_info in stakes_info {
            let delegation = stake_info
                .stake
                .delegation()
                .expect("Undelegated stake under control");

            if delegation.voter_pubkey == self.validator_vote
                && delegation.deactivation_epoch == std::u64::MAX
            {
                info!(
                    "unstake account {} {}",
                    stake_info.index, stake_info.record.stake_account
                );
                builder.emergency_unstake(
                    &marinade.state,
                    validator_manager_authority.clone(),
                    stake_info.record.stake_account,
                    stake_info.index,
                    validator_index as u32,
                )?;
            }
        }
        // process emergency_unstake transaction
        marinade
            .client
            .process_transaction_sequence(common.simulate, builder.combined_sequence())?;

        // if we also want to remove the validator from the list
        if self.remove {
            info!("removing validator {}", validator_index);
            // this is made in a new transaction, if removing validator is an error don't rollback unstakes
            builder.remove_validator(
                &marinade.state,
                validator_manager_authority,
                validator_index as u32,
                self.validator_vote,
            )?;
            marinade
                .client
                .process_transaction_sequence(common.simulate, builder.combined_sequence())?;
        } else {
            info!(
                "remove option not set, NOT removing validator {}",
                validator_index
            );
        }
        Ok(())
    }
}
