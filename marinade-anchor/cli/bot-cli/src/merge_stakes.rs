use crate::Common;
use anyhow::Result;
use cli_common::anchor_lang::prelude::*;
use cli_common::solana_sdk::{clock::Epoch, pubkey::Pubkey, sysvar::stake_history};
use cli_common::{
    instruction_helpers::InstructionHelpers, rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::RpcMarinade, transaction_builder::TransactionBuilder,
};
use log::{debug, info};
use std::time::SystemTime;

pub fn process(
    common: &Common,
    marinade: &RpcMarinade,
    builder: &mut TransactionBuilder,
    max_seconds: u64,
    //
) -> Result<()> {
    let (validator_list, _) = marinade.validator_list()?;
    let (stakes_info, _) = marinade.stakes_info()?;
    let stake_history: StakeHistory = bincode::deserialize(
        &marinade
            .client
            .get_account_data_retrying(&stake_history::ID)?,
    )?;
    let start = SystemTime::now();
    let clock = &marinade.get_clock()?;

    #[derive(PartialEq, Eq, Debug)]
    enum MergeableStakeKind {
        ActivationEpoch,
        FullyActive,
    }
    struct MergeableStakeInfo {
        index: u32,
        kind: MergeableStakeKind,
        credits_observed: u64,
        address: Pubkey,
        validator_index: u32,
    }
    let mergeable_stakes: Vec<MergeableStakeInfo> = stakes_info
        .iter()
        .filter_map(|stake_info| {
            stake_info.stake.stake().and_then(|stake| {
                if stake.delegation.deactivation_epoch == Epoch::MAX {
                    let (effective, activating, deactivating) = stake
                        .delegation
                        .stake_activating_and_deactivating(clock.epoch, Some(&stake_history));
                    let validator_index = validator_list
                        .iter()
                        .position(|validator| {
                            validator.validator_account == stake.delegation.voter_pubkey
                        })
                        .expect("Validator not found")
                        as u32;
                    if effective == 0 {
                        Some(MergeableStakeInfo {
                            index: stake_info.index,
                            kind: MergeableStakeKind::ActivationEpoch,
                            credits_observed: stake.credits_observed,
                            address: stake_info.record.stake_account,
                            validator_index,
                        })
                    } else if activating == 0 && deactivating == 0 {
                        Some(MergeableStakeInfo {
                            index: stake_info.index,
                            kind: MergeableStakeKind::FullyActive,
                            credits_observed: stake.credits_observed,
                            address: stake_info.record.stake_account,
                            validator_index,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        })
        .collect();
    info!(
        "Active & activating stake-accounts: {}",
        mergeable_stakes.len()
    );
    let mut count_tx_ok: u32 = 0;
    let mut count_tx_err: u32 = 0;
    let mut count_processed = 0;
    let mut exit_for = false;
    for source_index in (1..mergeable_stakes.len()).rev() {
        for destination_index in 0..source_index {
            if mergeable_stakes[destination_index].validator_index
                == mergeable_stakes[source_index].validator_index
            {
                //same validator
                debug!(
                    "Validator Index {}",
                    mergeable_stakes[destination_index].validator_index
                );
                debug!(
                    "Check Dest   {} credits_observed {} kind {:?}",
                    mergeable_stakes[destination_index].address,
                    mergeable_stakes[destination_index].credits_observed,
                    mergeable_stakes[destination_index].kind
                );
                debug!(
                    "Check Source {} credits_observed {} kind {:?}",
                    mergeable_stakes[source_index].address,
                    mergeable_stakes[source_index].credits_observed,
                    mergeable_stakes[source_index].kind
                );

                if mergeable_stakes[destination_index].credits_observed
                    == mergeable_stakes[source_index].credits_observed
                    && mergeable_stakes[destination_index].kind
                        == mergeable_stakes[source_index].kind
                {
                    info!(
                        "Merge {} <- {}",
                        mergeable_stakes[destination_index].address,
                        mergeable_stakes[source_index].address
                    );
                    builder.begin();
                    builder.merge_stakes(
                        &marinade.state,
                        mergeable_stakes[destination_index].address,
                        mergeable_stakes[destination_index].index,
                        mergeable_stakes[source_index].address,
                        mergeable_stakes[source_index].index,
                        mergeable_stakes[destination_index].validator_index,
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
                            info!("TX ERR {:?}", err);
                            count_tx_err += 1;
                        }
                    };
                    count_processed += 1;
                    break;
                } else {
                    debug!("mismatch")
                }
                debug!("-------")
            }
            if max_seconds > 0 {
                let elapsed_secs = start.elapsed().unwrap().as_secs();
                if elapsed_secs > max_seconds {
                    info!("{} seconds elapsed - breaking ", max_seconds);
                    exit_for = true;
                }
            }
            if exit_for {
                break;
            }
        }
        if exit_for {
            break;
        }
    }

    info!(
        "count processed:{}, tx Ok:{}, tx Err:{}",
        count_processed, count_tx_ok, count_tx_err
    );

    Ok(())
}
