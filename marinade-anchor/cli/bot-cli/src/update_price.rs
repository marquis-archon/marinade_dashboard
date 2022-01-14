// LMT- I would prefer the bot to processes *a single stake-account per tx*
// We keep calling this function util all stake-accounts have been processed
// each call computes rewards, updates mSOL price and separates 1% protocol fee
//
// Having single-action functions makes the entire system less efficient (read calls & tx) but more resilient.
// The idea is that you can call bot cranks in any order or at any time and it will be
// a small operation and will not corrupt state or block the system no matter the order of the calls

use crate::Common;
use anyhow::Result;
use cli_common::anchor_lang::prelude::*;
use cli_common::solana_sdk::{clock::Epoch, pubkey::Pubkey, sysvar::stake_history};
use cli_common::{
    instruction_helpers::InstructionHelpers, rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::RpcMarinade, transaction_builder::TransactionBuilder,
};
use log::{debug, info, warn};
use std::collections::HashMap;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct UpdatePriceOptions {}

impl UpdatePriceOptions {
    // fn update_price()
    // returns true if all stakes are processed
    pub fn process(
        self,
        common: &Common,
        marinade: &RpcMarinade,
        builder: &mut TransactionBuilder,
        //
        // returns Err if some unexpected error
        // returns Ok(true) if all transactions went ok
        // returns Ok(false) if some transactions failed
    ) -> Result<bool> {
        // get stake accounts list
        let (stakes_info, _) = marinade.stakes_info_reversed()?;

        let clock = marinade.get_clock()?;

        // make sure we have at least one account that needs update
        {
            let mut some_needs_update: bool = false;
            for stake_account in &stakes_info {
                if stake_account.record.last_update_epoch < clock.epoch {
                    some_needs_update = true;
                    break;
                }
            }
            if !some_needs_update {
                info!("no stake-account needs update");
                return Ok(true);
            }
        }

        // get validator list
        let (validator_list, _) = marinade.validator_list()?;

        //create a HashMap: ValidatorPubKey => validator_index
        let validator_indices: HashMap<Pubkey, u32> = validator_list
            .iter()
            .enumerate()
            .map(|(index, validator)| (validator.validator_account, index as u32))
            .collect();

        let stake_history: StakeHistory = bincode::deserialize(
            &marinade
                .client
                .get_account_data_retrying(&stake_history::ID)?,
        )?;

        let mut count_tx_ok: u32 = 0;
        let mut count_tx_err: u32 = 0;
        let mut count_processed: u32 = 0;
        //let mut all_stakes_updated = true; // Will be set to false when we reject to put some stake needed for update into transaction

        //for each stake account
        for stake_info in &stakes_info {
            // if already processed, skip
            if stake_info.record.last_update_epoch == clock.epoch {
                continue;
            }
            // get delegation state
            let delegation = stake_info
                .stake
                .delegation()
                .expect("Undelegated stake under control");
            let (effective, _activating, _deactivating) =
                delegation.stake_activating_and_deactivating(clock.epoch, Some(&stake_history));

            // if active
            if delegation.deactivation_epoch == Epoch::MAX {
                // if already visited, skip
                if delegation.stake == stake_info.record.last_update_delegated_lamports {
                    debug!(
                        "Stake {}: update is not needed",
                        stake_info.record.stake_account
                    );
                    continue;
                }
                info!(
                    "Update active {}, delegated to {}",
                    stake_info.record.stake_account, &delegation.voter_pubkey
                );
                let validator_index = *validator_indices
                    .get(&delegation.voter_pubkey)
                    .expect("Unknown validator");
                info!(
                    " -- stake_info.index {}, validator_index {}",
                    stake_info.index, validator_index
                );
                builder.begin();
                builder.update_active(
                    &marinade.state,
                    stake_info.record.stake_account,
                    stake_info.index,
                    validator_index,
                );
                builder.commit();
                match marinade
                    .client
                    .process_transaction(common.simulate, builder.build_one())
                {
                    Ok(_) => {
                        count_tx_ok += 1;
                    }
                    Err(err) => {
                        // just show the err, count it, and continue with next account
                        // account will be retried on next run of do-work in 5 minutes
                        warn!("TX ERR {:?}", err);
                        count_tx_err += 1;
                    }
                };
            //
            // if deactivated
            } else if effective == 0 {
                info!("Update deactivated {}", stake_info.record.stake_account);
                builder.begin();
                builder.update_deactivated(
                    &marinade.state,
                    stake_info.record.stake_account,
                    stake_info.index,
                );
                builder.commit();
                match marinade
                    .client
                    .process_transaction(common.simulate, builder.build_one())
                {
                    Ok(_) => {
                        count_tx_ok += 1;
                    }
                    Err(err) => {
                        // just show the err, count it, and continue with next account
                        // account will be retried on next run of do-work in 5 minutes
                        warn!("TX ERR {:?}", err);
                        count_tx_err += 1;
                    }
                };
            //
            // assume cooling-down
            } else {
                warn!("Updating cooling down stakes is not supported");
                /* TODO:
                // if no change since last visit, skip
                if delegation.stake == stake_info.record.last_update_delegated_lamports {
                    debug!(
                        "Stake {}: update is not needed",
                        stake_info.record.stake_account
                    );
                    continue;
                }
                let withdraw_amount = stake_info.balance
                    - stake_info.stake.meta().unwrap().rent_exempt_reserve
                    - effective;
                info!(
                    "Update cooling down {}: withdraw {}",
                    stake_info.record.stake_account, withdraw_amount
                );
                builder.begin();
                builder.update_cooling_down(
                    &marinade.state,
                    stake_info.record.stake_account,
                    stake_info.index,
                    withdraw_amount,
                );
                builder.commit();
                match marinade
                    .client
                    .process_transaction(common.simulate, builder.build_one())
                {
                    Ok(_) => {
                        count_tx_ok += 1;
                    }
                    Err(err) => {
                        // just show the err, count it, and continue with next account
                        // account will be retried on next run of do-work in 5 minutes
                        warn!("TX ERR {:?}", err);
                        count_tx_err += 1;
                    }
                };

                */
            }

            // one more processed
            count_processed += 1;

            // // But if it not fit tx or instruction count > limit we must rollback it
            // if builder.fit_into_single_transaction()
            //     && (common.limit == 0 || count_processed <= common.limit)
            // {
            //     builder.commit();
            //     info!("builder.commit()");
            // } else {
            //     info!("There are instructions not fit in single transaction");
            //     // Undo last command
            //     builder.rollback();
            //     if common.limit > 0 && count_processed > common.limit {
            //         info!("limit of {} reached", common.limit);
            //     }
            //     all_stakes_updated = false;

            //     break;
            // }
        }

        info!(
            "count processed:{}, tx Ok:{}, tx Err:{}",
            count_processed, count_tx_ok, count_tx_err
        );
        // info!("count_processed:{} builder.is_empty:{}", count_processed, builder.is_empty());
        // if builder.is_empty() {
        //     return Ok(all_stakes_updated); // No stakes left not updated
        // }
        // marinade
        //     .client
        //     .process_transaction(common.simulate, builder.build_one())?;

        Ok(count_tx_err == 0)
    }
}
