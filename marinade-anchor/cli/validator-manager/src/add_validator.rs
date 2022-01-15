use crate::Common;

use anyhow::bail;
use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::solana_sdk::{pubkey::Pubkey, signature::Signer, system_program};
use cli_common::{
    instruction_helpers::InstructionHelpers, rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::RpcMarinade, transaction_builder::TransactionBuilder,
};
use log::{error, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct AddValidatorOptions {
    validator_votes: Vec<Pubkey>,
}

impl AddValidatorOptions {
    pub fn process(self, common: Common, client: Arc<RpcClient>) -> anyhow::Result<()> {
        info!("Add validators: {:?}", self.validator_votes);

        let mut builder = TransactionBuilder::limited(common.fee_payer.as_keypair());

        let marinade = RpcMarinade::new(client, &common.instance.as_pubkey())?;

        let rent_payer = if let Some(rent_payer) = common.rent_payer {
            info!("Use rent payer = {}", rent_payer);
            rent_payer.as_keypair()
        } else {
            info!("Use fee payer as rent payer");
            common.fee_payer.as_keypair()
        };
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

        let (current_validators, max_validators) = marinade.validator_list()?;
        let validator_indices: HashMap<Pubkey, usize> = current_validators
            .iter()
            .enumerate()
            .map(|(index, validator)| (validator.validator_account, index))
            .collect();
        let mut added_validator_count = 0;

        let mut add_validator = |key: Pubkey| -> anyhow::Result<bool> {
            if let Some(index) = validator_indices.get(&key) {
                if current_validators[*index].score == 0 {
                    builder.set_validator_score(
                        &marinade.state,
                        validator_manager_authority.clone(),
                        *index as u32,
                        key,
                        0x100,
                    )?;
                } else {
                    warn!("Validator {} is already added", key);
                }
            } else {
                if current_validators.len() + added_validator_count >= max_validators as usize {
                    warn!(
                        "Can not add validator {} because max validator count is reached",
                        key
                    );
                    return Ok(false);
                }
                builder.add_validator(
                    &marinade.state,
                    validator_manager_authority.clone(),
                    key,
                    0x100,
                    rent_payer.clone(),
                )?; // TODO: input score
                added_validator_count += 1;
            }
            Ok(true)
        };

        if self.validator_votes.is_empty() {
            for validator_to_add in marinade.client.get_vote_accounts()?.current {
                // TODO: check validator.commission and other fields
                match validator_to_add.vote_pubkey.parse::<Pubkey>() {
                    Ok(key) => {
                        add_validator(key)?;
                    }
                    Err(err) => {
                        error!("Parse validator pubkey error {}", err);
                        bail!("Parse validator pubkey error {}", err)
                    }
                }
            }
        } else {
            // TODO: check accounts data
            for key in &self.validator_votes {
                add_validator(*key)?;
            }
        }

        marinade
            .client
            .process_transaction_sequence(common.simulate, builder.combined_sequence())?;
        Ok(())
    }
}
