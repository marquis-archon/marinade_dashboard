// LMT- I would prefer the bot processes *a single validator* (the one with max delta target)
// and sends a single transaction staking & updating validator state.
// We keep calling this function util state.stake_total() >= self.validator_system.total_balance
// This single-action functions makes the entire system less efficient (read calls) but more resilient.
// The idea is that you can call bot cranks in any order or at any time and it will be
// a small operation and will not corrupt state no matter the order of the calls

use crate::Common;
use anyhow::{bail, Result};
use cli_common::marinade_finance::{state::StateHelpers, validator_system::ValidatorRecord};
use cli_common::solana_sdk::{
    native_token::lamports_to_sol,
    rent::Rent,
    signature::{Keypair, Signer},
    stake::{self, state::StakeState},
    system_program,
    sysvar::rent,
};
use cli_common::{
    instruction_helpers::InstructionHelpers,
    rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::{RpcMarinade, StakeInfo},
    transaction_builder::TransactionBuilder,
    InputKeypair,
};
use log::{error, info, warn};
use std::time::SystemTime;
use std::{convert::TryInto, sync::Arc, thread, time};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub struct StakeDeltaOptions {
    #[structopt(short = "r", name = "rent-payer")]
    pub rent_payer: Option<InputKeypair>,
}

impl StakeDeltaOptions {
    // return true if it is impossible to progress anymore
    pub fn process(
        self,
        common: &Common,
        marinade: &mut RpcMarinade,
        builder: &mut TransactionBuilder,
        start: &SystemTime,
        max_run_seconds: u32, // do not run more than max_run_seconds from start
                              //
                              // returns Err if some unexpected error
                              // returns Ok(true) if all transactions went ok
                              // returns Ok(false) if some transactions failed
    ) -> Result<bool> {
        let rent_payer = if let Some(rent_payer) = self.rent_payer {
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
        builder.add_signer(rent_payer.clone());
        let rent: Rent = bincode::deserialize(&marinade.client.get_account_data(&rent::id())?)?;
        let clock = &marinade.get_clock()?;

        let wait_between_transactions = time::Duration::from_secs(5);

        let reserve_balance = marinade
            .client
            .get_balance(&marinade.state.reserve_address())?;

        let total_stake_delta: i128 = marinade.state.stake_delta(reserve_balance);
        // stake_delta:i128 = reserve_balance - rent_exempt_for_token_acc
        //  + self.stake_system.total_cooling_down
        //  - self.circulating_ticket_balance
        // meaning:
        // reserve_balance - rent_exempt_for_token_acc // All sol in reserve_pda (minus rent)
        // + total_cooling_down  // plus all SOL "in their way" to reserve_pda.
        //                       // total_cooling_down is incr. when accounts are deactivated, and decr. when the SOL is sent to reserve_pda
        // - circulating_ticket_balance // minus all SOL that must be in reserve PDA for users' delayed-unstake tickets.
        //                              // circulating_ticket_balance is incr. when tickets are created and decr. when tickets are claimed
        //
        // if the result is *positive*, means we've more SOL than required to cover ticket claims, so we need to *stake* the excess
        // if the result is *negative*, means we've less SOL than required to cover ticket claims, so we need to *unstake* some more

        if total_stake_delta == 0
            || (total_stake_delta >= 0
                && total_stake_delta < marinade.state.stake_system.min_stake as i128)
        {
            warn!("stake_delta = {}. Nothing to do", total_stake_delta);
            return Ok(true);
        }

        // compute stake_target as currently staked +/- delta
        let total_stake_target: u64 =
            (marinade.state.validator_system.total_active_balance as i128 + total_stake_delta)
                .try_into()?;

        // create validators_info vec
        struct ValidatorInfo {
            index: u32,
            record: ValidatorRecord,
            stake_delta: i128, // delta between actual stake and target stake according to score, positive=>need more stake, negative=>needs unstake
        }
        let (validator_list, _) = marinade.validator_list()?;
        let mut validators_info: Vec<ValidatorInfo> = validator_list
            .into_iter()
            .enumerate()
            .map(|(index, record)| {
                let stake_target = marinade
                    .state
                    .validator_system
                    .validator_stake_target(&record, total_stake_target)
                    .unwrap();
                ValidatorInfo {
                    index: index as u32,
                    record,
                    stake_delta: stake_target as i128 - record.active_balance as i128,
                }
            })
            .collect();
        //
        // loop over validators to reach total_stake_target (either staking or unstaking)
        //
        info!(
            "About to loop over {} validators...",
            &validators_info.len()
        );
        let mut count_tx_ok: u32 = 0;
        let mut count_tx_err: u32 = 0;
        let mut count_processed = 0;
        if total_stake_delta > 0 {
            // ----------------
            // we need to stake
            // ----------------
            info!(
                "to stake additional {} SOL, {} lamports",
                lamports_to_sol(total_stake_delta as u64),
                total_stake_delta
            );
            // sort validators to process first the one with most score
            validators_info.sort_by_key(|info| -(info.record.score as i64));
            //execute at most self.limit staking instructions, one per validator
            //starting with the better scored
            for validator_info in &validators_info {
                if validator_info.stake_delta < 0 {
                    continue; // this validator does not need stake
                }
                if validator_info.stake_delta < marinade.state.stake_system.min_stake as i128 {
                    continue; // we don't move less than marinade.state.min_stake (1 SOL)
                }
                if validator_info.record.last_stake_delta_epoch == clock.epoch {
                    // check if we have some extra stake runs allowed
                    if marinade.state.stake_system.extra_stake_delta_runs == 0 {
                        info!(
                            "Validator {} has stake-delta already run in this epoch",
                            validator_info.record.validator_account
                        );
                        continue;
                    } else {
                        info!(
                            "using one of {} extra_stake_delta_runs",
                            marinade.state.stake_system.extra_stake_delta_runs
                        );
                    }
                }

                // ---------------------------
                // recompute total_stake_delta
                // ---------------------------
                let reserve_balance = marinade
                    .client
                    .get_balance(&marinade.state.reserve_address())?;
                let total_stake_delta: i128 = marinade.state.stake_delta(reserve_balance);
                if total_stake_delta <= 0
                    || (total_stake_delta > 0
                        && total_stake_delta < marinade.state.stake_system.min_stake as i128)
                {
                    warn!(
                        "total_stake_delta is now = {}. Nothing to do",
                        total_stake_delta
                    );
                    break;
                }
                // ---------------------

                let to_stake = std::cmp::min(validator_info.stake_delta, total_stake_delta) as u64;
                info!(
                    "Stake {} ({} SOL) into validator {}",
                    to_stake,
                    lamports_to_sol(to_stake),
                    validator_info.record.validator_account
                );
                //note: amount is not sent in the instruction, it's recomputed by the on-chain program
                builder.begin();
                // create a new stake-account
                let stake_keypair = Arc::new(Keypair::new());
                builder.create_account(
                    stake_keypair.clone(),
                    std::mem::size_of::<StakeState>(),
                    &stake::program::ID,
                    &rent,
                    "stake_account",
                )?;
                // stake into the account
                builder.stake_reserve(
                    &marinade.state,
                    validator_info.index,
                    validator_info.record.validator_account,
                    stake_keypair.pubkey(),
                );
                // temp-fix, execute one tx per validator
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
                //wait 3 secs between transactions to not saturate the RPC
                thread::sleep(wait_between_transactions);
                // update state
                marinade.update()?;

                // if builder.fit_into_single_transaction() {
                //     builder.commit();
                // } else {
                //     info!("There are instructions not fit in single transaction");
                //     // Undo last command
                //     builder.rollback(); // remove last transaction because it is not fix into tx
                //     break; // Stop adding instructions
                // }

                count_processed += 1;
                if common.limit > 0 && count_processed >= common.limit {
                    info!("limit of {} reached", common.limit);
                    break;
                }
                let elapsed_seconds = start.elapsed().unwrap().as_secs();
                if elapsed_seconds > max_run_seconds as u64 {
                    info!("limit of {} run seconds reached", max_run_seconds);
                    break;
                }
                //
            } // loop
              //
        } else {
            // -------
            // unstake
            // -------
            let total_to_unstake = -total_stake_delta as u64;
            info!("to unstake {} SOL", lamports_to_sol(total_to_unstake));
            // sort validators to process first the one requiring the most unstake (info.stake_delta is negative)
            validators_info.sort_by_key(|info| info.stake_delta);

            let (stakes_info, _max_stake_count /*TODO*/) = marinade.stakes_info_reversed()?;

            let mut sum_maybe_deactivated_total = 0;
            for validator_info in &validators_info {
                if sum_maybe_deactivated_total >= total_to_unstake {
                    info!(
                        "target reached {}",
                        lamports_to_sol(sum_maybe_deactivated_total)
                    );
                    break;
                }
                if validator_info.stake_delta >= 0 {
                    info!(
                        "rest of the list are validators requiring stake ({})",
                        validator_info.stake_delta
                    );
                    break;
                }

                // ---------------------
                // recompute stake_delta
                // ---------------------
                let reserve_balance = marinade
                    .client
                    .get_balance(&marinade.state.reserve_address())?;
                let total_stake_delta: i128 = marinade.state.stake_delta(reserve_balance);
                if total_stake_delta >= 0 {
                    warn!("stake_delta is now = {}. Nothing to do", total_stake_delta);
                    break;
                }
                let total_to_unstake = -total_stake_delta as u64;
                // ---------------------

                let mut to_unstake_from_validator = -validator_info.stake_delta as u64;
                if to_unstake_from_validator > total_to_unstake {
                    to_unstake_from_validator = total_to_unstake;
                }
                info!(
                    "Unstake {} ({} SOL) from validator {:?}",
                    to_unstake_from_validator,
                    lamports_to_sol(to_unstake_from_validator),
                    validator_info.record
                );
                if validator_info.record.last_stake_delta_epoch == clock.epoch {
                    info!(
                        "--- WARN: Validator {} has stake-delta already run in this epoch",
                        validator_info.record.validator_account
                    );
                    continue;
                }

                let mut validator_stakes: Vec<&StakeInfo> = stakes_info
                    .iter()
                    .filter(|stake| {
                        if let Some(delegation) = stake.stake.delegation() {
                            delegation.voter_pubkey == validator_info.record.validator_account
                                && delegation.deactivation_epoch == u64::MAX
                        } else {
                            false
                        }
                    })
                    .collect();
                // Try to kill smallest stakes first (if they are not merged yet)
                // because it is better to deactivate whole stake account than split-deactivate
                validator_stakes
                    .sort_by_key(|stake_info| stake_info.stake.delegation().unwrap().stake);
                let mut sum_maybe_deactivated_this_validator = 0;

                // let mut transaction_overflow = false;
                let mut left_to_unstake_from_validator = to_unstake_from_validator;
                for stake_info in validator_stakes {
                    if sum_maybe_deactivated_this_validator >= to_unstake_from_validator {
                        break; // target was reached
                    }
                    builder.begin();
                    let split_stake_keypair = Arc::new(Keypair::new());
                    // commented, no need to create split-stake account
                    // it can be done on-chain. We only send new keypair
                    // builder.create_account(
                    //     split_stake_keypair.clone(),
                    //     std::mem::size_of::<StakeState>(),
                    //     &stake::program::ID,
                    //     &rent,
                    //     "split_stake_account",
                    // )?;
                    builder.deactivate_stake(
                        &marinade.state,
                        stake_info.record.stake_account,
                        split_stake_keypair,
                        rent_payer.clone(),
                        stake_info.index,
                        validator_info.index,
                    );
                    // execute one tx per account
                    builder.commit();
                    match marinade
                        .client
                        .process_transaction(common.simulate, builder.build_one())
                    {
                        Ok(_) => {
                            // only add to sum_maybe_deactivated_this_validator
                            // if the tx went ok. If it was an already deactivating account (emergency-unstake)
                            // the unstake instruction might fail
                            let estimated_unstaked =
                                if stake_info.record.last_update_delegated_lamports
                                    < left_to_unstake_from_validator
                                {
                                    // if the stake account was smaller than the amount we wanted to unstake
                                    // all the account
                                    stake_info.record.last_update_delegated_lamports
                                } else {
                                    // unstaked only what was left (account was splitted)
                                    left_to_unstake_from_validator
                                };
                            sum_maybe_deactivated_this_validator += estimated_unstaked;
                            left_to_unstake_from_validator =
                                left_to_unstake_from_validator.saturating_sub(estimated_unstaked);
                            count_tx_ok += 1;
                        }
                        Err(err) => {
                            // just show the err, count it, and continue with next account
                            // account will be retried on next run of do-work in 5 minutes
                            info!("TX ERR {:?}", err);
                            count_tx_err += 1;
                        }
                    };
                    //wait 3 secs between transactions to not saturate the RPC
                    thread::sleep(wait_between_transactions);
                    // update state
                    marinade.update()?;

                    // if builder.fit_into_single_transaction() {
                    //     builder.commit();
                    // } else {
                    //     info!("There are instructions not fit in single transaction");
                    //     // Undo last command
                    //     builder.rollback(); // remove last transaction because it is not fix into tx
                    //     transaction_overflow = true; // Stop looping over validators
                    //     break; // Stop looping over stakes
                    // }
                    count_processed += 1;
                    if common.limit > 0 && count_processed >= common.limit {
                        break;
                    }
                }
                sum_maybe_deactivated_total += sum_maybe_deactivated_this_validator;

                // if transaction_overflow {
                //     break;
                // }
                if common.limit > 0 && count_processed >= common.limit {
                    info!("limit of {} reached", common.limit);
                    break;
                }
                let elapsed_seconds = start.elapsed().unwrap().as_secs();
                if elapsed_seconds > max_run_seconds as u64 {
                    info!("limit of {} run seconds reached", max_run_seconds);
                    break;
                }
            }
        }

        info!(
            "count processed:{}, tx Ok:{}, tx Err:{}",
            count_processed, count_tx_ok, count_tx_err
        );

        // if builder.is_empty() {
        //     // We can not do anything. Wait for the next epoch
        //     warn!("Can not fully apply delta. Wait for the next epoch");
        //     return Ok(true);
        // }

        // marinade
        //     .client
        //     .process_transaction(common.simulate, builder.build_one())?;

        Ok(count_tx_err == 0) // run this again to ensure all work is done
    }
}
