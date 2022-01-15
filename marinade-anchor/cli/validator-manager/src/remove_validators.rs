use crate::Common;

use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::solana_sdk::pubkey::Pubkey;
use cli_common::{
    instruction_helpers::InstructionHelpers, rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::RpcMarinade, transaction_builder::TransactionBuilder,
};
use log::{error, info, warn};

use std::{collections::HashSet, sync::Arc};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct RemoveValidatorsOptions {
    validator_votes: Vec<Pubkey>,
}

impl RemoveValidatorsOptions {
    pub fn process(self, common: Common, client: Arc<RpcClient>) -> anyhow::Result<()> {
        info!("Remove validators: {:?}", self.validator_votes);

        let mut builder = TransactionBuilder::limited(common.fee_payer.as_keypair());

        let marinade = RpcMarinade::new(client, &common.instance.as_pubkey())?;

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

        let (current_validators, _max_validators) = marinade.validator_list()?;

        let validators_to_remove = if self.validator_votes.is_empty() {
            current_validators
                .into_iter()
                .enumerate()
                .filter(|(_index, validator)| validator.active_balance == 0)
                .rev() // to make remove index stable operation
                .collect::<Vec<_>>()
        } else {
            let mut to_remove = HashSet::new();
            to_remove.extend(self.validator_votes.into_iter());

            let validators_to_remove = current_validators
                .into_iter()
                .enumerate()
                .filter(|(_index, validator)| {
                    if to_remove.remove(&validator.validator_account) {
                        if validator.active_balance > 0 {
                            error!(
                                "Can not remove validator {} with balance {}",
                                validator.validator_account, validator.active_balance
                            );
                            false
                        } else {
                            true
                        }
                    } else {
                        false
                    }
                })
                .rev() // to make remove index stable operation
                .collect::<Vec<_>>();

            for validator in to_remove {
                warn!("Unknown validator {}", validator);
            }

            validators_to_remove
        };

        for (index, validator) in validators_to_remove {
            builder.remove_validator(
                &marinade.state,
                validator_manager_authority.clone(),
                index as u32,
                validator.validator_account,
            )?;
        }

        marinade
            .client
            .process_transaction_sequence(common.simulate, builder.combined_sequence())?;
        Ok(())
    }
}
