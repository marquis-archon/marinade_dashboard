use crate::Common;
use anyhow::bail;
use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::solana_sdk::{
    native_token::{lamports_to_sol, LAMPORTS_PER_SOL},
    pubkey::Pubkey,
    signature::Signer,
    system_program,
};
use cli_common::{
    instruction_helpers::InstructionHelpers,
    marinade_finance::state::StateHelpers,
    rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::{RpcMarinade, StakeInfo},
    transaction_builder::TransactionBuilder,
    Cluster,
};
use log::{debug, error, info, warn};

use std::{collections::HashMap, str::FromStr};

use std::sync::Arc;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct UpdateScoresOptions {
    #[structopt(
        short = "f",
        env = "SCORES_FILE",
        long = "scores-file",
        help = "csv file from score-post-process"
    )]
    scores_file: Option<String>,

    #[structopt(long = "show-changes", help = "show changed records")]
    show_changes: bool,

    #[structopt(long = "show-all", help = "show all records even if no change")]
    show_all: bool,

    #[structopt(short = "1", help = "send transactions one by one")]
    one_by_one: bool,

    #[structopt(short = "n", help = "no control report")]
    no_control_report: bool,
}

// data from post-process.csv
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
struct ValidatorScore {
    epoch: u64,
    rank: u32,
    score: u32,
    name: String,
    credits_observed: u64,
    vote_address: String,
    commission: u8,
    average_position: f64,
    data_center_concentration: f64,
    avg_active_stake: f64,
    apy: Option<f64>,
    delinquent: bool,
    this_epoch_credits: u64,
    pct: f64,
    marinade_staked: f64,
    should_have: f64,
    remove_level: u8,
    remove_level_reason: String,
    under_nakamoto_coefficient: bool,
    keybase_id: String,
    identity: String,
    stake_concentration: f64,
    base_score: u64,
}

impl UpdateScoresOptions {
    pub fn process(
        self,
        common: Common,
        client: Arc<RpcClient>,
        _cluster: Cluster,
    ) -> anyhow::Result<()> {
        //
        //read file post-process.csv into validator_scores:Vec
        let mut validator_scores: Vec<ValidatorScore> = Vec::with_capacity(2000);
        let scores_file = &self.scores_file.unwrap_or("./post-process.csv".into());
        {
            println!("# Update Scores from {}", scores_file);
            let mut rdr = csv::Reader::from_path(scores_file)?;
            for result in rdr.deserialize() {
                let record: ValidatorScore = result?;
                validator_scores.push(record);
            }
        }
        // sort validator_scores by score desc
        validator_scores.sort_by(|a, b| b.score.cmp(&a.score));

        let total_score: u64 = validator_scores.iter().map(|s| s.score as u64).sum();

        //prepare txn builder
        let mut builder = TransactionBuilder::limited(common.fee_payer.as_keypair());

        let marinade = RpcMarinade::new(client, &common.instance.as_pubkey())?;

        let rent_payer = if let Some(rent_payer) = common.rent_payer {
            println!("# Use rent payer = {}", rent_payer);
            rent_payer.as_keypair()
        } else {
            println!("# Use fee payer as rent payer");
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
                println!(
                    "# Using validator manager authority {}",
                    validator_manager_authority
                );
                validator_manager_authority.as_keypair()
            } else {
                println!("# Using fee payer as validator manager authority");
                common.fee_payer.as_keypair()
            };

        let (current_validators, max_validators) = marinade.validator_list()?;
        println!(
            "# Marinade on chain register: {} Validators of {} max capacity, total_score {}",
            current_validators.len(),
            max_validators,
            total_score
        );
        // get also stake accounts
        let (stakes, _max_stakes) = marinade.stakes_info()?;
        // create a hashmap PubKey->index
        let validator_indices: HashMap<Pubkey, usize> = current_validators
            .iter()
            .enumerate()
            .map(|(index, validator)| (validator.validator_account, index))
            .collect();

        // process all records in the csv
        println!(
            "# Processing {} records in {} file",
            validator_scores.len(),
            &scores_file
        );
        let mut validators_with_stake = 0;
        let mut validators_with_score = 0;
        let mut added_validator_count = 0;
        let mut skipped_validator_count = 0;
        let mut updated_validator_count = 0;
        let mut add_validator_error_count = 0;
        let mut removed_count = 0;
        let mut total_unstake_under_nakamoto_coefficient: f64 = 0.0;

        // ----------------------------
        // set score on chain
        // ----------------------------
        for v in validator_scores.iter_mut() {
            if v.score > 0 {
                validators_with_score += 1
            };
            if v.marinade_staked > 0.0 {
                validators_with_stake += 1;
            }
            let vote = Pubkey::from_str(&v.vote_address)?;
            if let Some(index) = validator_indices.get(&vote) {
                // it is in the list

                // show only changes in simulation mode
                let show_info = self.show_all
                    || (self.show_changes && v.score != current_validators[*index].score)
                    || (self.show_changes && v.remove_level > 0);

                if show_info {
                    println!("# ------------------------------------");
                    println!(
                        "#{} {}) {} credits:{} dcc:{} {} {}",
                        v.rank,
                        index,
                        v.score,
                        v.credits_observed,
                        v.data_center_concentration,
                        v.name,
                        v.vote_address
                    );
                    println!("# has {} SOL staked ", v.marinade_staked);
                }

                // has the score changed +/-5%?
                // note considering +/-5% instead of exact value reduces update score transactions 75%
                //if v.score != current_validators[*index].score {
                let cur_score = current_validators[*index].score;
                if v.score < (cur_score as u64 * 95 / 100) as u32
                    || v.score > (cur_score as u64 * 105 / 100) as u32
                {
                    // the score has changed +/- 5%, update
                    if show_info {
                        println!(
                            "# CHANGE score {} -> {} {}% for {:?}",
                            cur_score,
                            v.score,
                            (v.score as u64 * 10000 / total_score) as f64 / 100.0,
                            v
                        )
                    }
                    builder.set_validator_score(
                        &marinade.state,
                        validator_manager_authority.clone(),
                        *index as u32,
                        Pubkey::from_str(&v.vote_address)?,
                        v.score,
                    )?;
                    updated_validator_count += 1;
                    if self.one_by_one {
                        marinade.client.process_transaction_sequence(
                            common.simulate,
                            builder.combined_sequence(),
                        )?;
                    }
                } else {
                    // change is less than 5% - ignore change
                    v.score = cur_score;
                }
                if v.remove_level >= 2 {
                    if v.marinade_staked == 0.0 {
                        if v.remove_level >= 3 || v.avg_active_stake < 100.0 {
                            println!("# -------------------------");
                            println!("# UNHEALTHY level {}: {:?} ", v.remove_level, v);
                            println!("# Reason {}", v.remove_level_reason);
                            println!("# --REMOVING FROM LIST");
                            removed_count += 1;
                            builder.remove_validator(
                                &marinade.state,
                                validator_manager_authority.clone(),
                                *index as u32,
                                Pubkey::from_str(&v.vote_address)?,
                            )?;
                            if self.one_by_one {
                                marinade.client.process_transaction_sequence(
                                    common.simulate,
                                    builder.combined_sequence(),
                                )?;
                            }
                        };
                    } else {
                        // we have stake there
                        println!("# -------------------------");
                        println!("# UNHEALTHY level {}: {:?} ", v.remove_level, v);
                        println!(
                            "# Reason {}, apy:{}, marinade-pct:{}",
                            v.remove_level_reason,
                            v.apy.unwrap_or(0.0),
                            (v.marinade_staked / v.avg_active_stake * 100.0) as u64
                        );
                        if v.remove_level >= 2 {
                            if v.under_nakamoto_coefficient {
                                total_unstake_under_nakamoto_coefficient += v.marinade_staked
                            };
                            println!("./validator-manager emergency-unstake {}", v.vote_address);
                            println!("# --- **** ---- **** ----------------");
                            println!(
                                "# --- HAS {} STAKED. DO manual-unstake --",
                                v.marinade_staked
                            );
                            println!("# --- **** ---- **** ----------------");
                        }
                    }
                }

            // else is not in the list
            } else {
                if v.remove_level > 0 {
                    debug!(
                        "Not healthy level {}: {}, validator {:?}",
                        v.remove_level, v.remove_level_reason, v
                    );
                }
                if v.remove_level >= 3 || (v.remove_level > 1 && v.avg_active_stake < 100.00) {
                    // very unhealthy or unhealthy & not enough stake, do not allow deposits (do not add to the list)
                    skipped_validator_count += 1;
                } else {
                    // add it
                    if current_validators.len() + added_validator_count >= max_validators as usize {
                        warn!(
                            "Can not add validator {} because max validator count is reached",
                            v.vote_address
                        );
                        add_validator_error_count += 1;
                    } else {
                        builder.add_validator(
                            &marinade.state,
                            validator_manager_authority.clone(),
                            Pubkey::from_str(&v.vote_address)?,
                            v.score,
                            rent_payer.clone(),
                        )?; // TODO: input score
                        added_validator_count += 1;
                        if self.show_changes {
                            println!("# Adding {:?}", v);
                            println!("# sending transactions");
                        }
                        if self.one_by_one {
                            marinade.client.process_transaction_sequence(
                                common.simulate,
                                builder.combined_sequence(),
                            )?;
                        }
                    }
                }
            }
        }

        // remove validators in marinade's on-chain list that have NO score
        info!("Removing validators having NO score");
        // process all records in current_validators
        for cv in current_validators {
            let val_vote = cv.validator_account.to_string();
            if !validator_scores.iter().any(|s| s.vote_address == val_vote) {
                // it was NOT processed in the previous loop
                // get stakes
                let validator_stakes: Vec<&StakeInfo> = stakes
                    .iter()
                    .filter(|stake| {
                        if let Some(delegation) = stake.stake.delegation() {
                            // Only active stakes
                            delegation.deactivation_epoch == u64::MAX
                                && delegation.voter_pubkey == cv.validator_account
                        } else {
                            false
                        }
                    })
                    .collect();
                let sum_stake: u64 = validator_stakes
                    .iter()
                    .map(|s| s.record.last_update_delegated_lamports)
                    .sum();

                println!("# Should not be in the list: {:?} ", cv);
                println!("# Reason: NOT EVEN IN THE SCORE FILE");
                if sum_stake == 0 {
                    println!("# --REMOVING");
                    removed_count += 1;
                    let index = validator_indices.get(&cv.validator_account).unwrap();
                    builder.remove_validator(
                        &marinade.state,
                        validator_manager_authority.clone(),
                        *index as u32,
                        cv.validator_account,
                    )?;
                    if self.one_by_one {
                        marinade.client.process_transaction_sequence(
                            common.simulate,
                            builder.combined_sequence(),
                        )?;
                    }
                } else {
                    total_unstake_under_nakamoto_coefficient += lamports_to_sol(sum_stake);
                    println!("./validator-manager emergency-unstake {}", val_vote);
                    println!("# --- **** ---- **** ----------------");
                    println!(
                        "# HAS {} STAKED. DO manual-unstake --",
                        lamports_to_sol(sum_stake)
                    );
                    println!("# --- **** ---- **** ----------------");
                }
            }
        }

        println!("# {} validators with stake>0, {} validators with score>0, {} validators added, {} skipped, {} updated, {} add errors, {} removed",
            validators_with_stake,
            validators_with_score,
            added_validator_count,
            skipped_validator_count,
            updated_validator_count,
            add_validator_error_count,
            removed_count
        );
        println!(
            "total_unstake_under_nakamoto_coefficient {}",
            total_unstake_under_nakamoto_coefficient,
        );

        if self.no_control_report == false {
            println!("# -------------");
            println!("# -- CONTROL --");
            println!("# -- SORTED by #Rank, first the ones requiring stake, then the ones requiring unstake");
            println!("# -------------");
            let reserve_address = marinade.state.reserve_address();
            let reserve_balance = marinade.client.get_balance(&reserve_address)?;
            let stake_delta =
                marinade.state.stake_delta(reserve_balance) / LAMPORTS_PER_SOL as i128;
            println!("#  stake-delta {}", stake_delta);

            // sort validator_scores by score desc (for stake)
            // and after that, by unbalance for unstake
            validator_scores.sort_by_key(|a| {
                if a.should_have > 0.0 && a.should_have > a.marinade_staked {
                    -(a.score as i64)
                } else {
                    (a.marinade_staked - a.should_have) as i64
                }
            });

            // list, theoretically this is what the bot will do
            let mut index = 0;
            let mut acum_stake: i128 = 0;
            for v in validator_scores {
                if v.marinade_staked > 0.0 || v.score > 0 {
                    let pct = (v.score as u64 * 10000 / total_score) as f64 / 100.0;
                    let to_stake = (v.should_have - v.marinade_staked) as i128;
                    acum_stake = acum_stake + to_stake;
                    println!(
                    "#{:4} {:4}) {:8} {:6}% m.stk:{:8} should:{:8} next:{:6} acum:{:6} credits:{:6} apy:{:.2} cm:{:2} dcc:{:7.4} {} {} stk:{}",
                    v.rank,
                    index,
                    v.score,
                    pct,
                    v.marinade_staked as u64,
                    v.should_have as u64,
                    to_stake,
                    acum_stake,
                    v.credits_observed,
                    v.apy.unwrap_or(-1.0),
                    v.commission,
                    v.data_center_concentration,
                    v.name,
                    v.vote_address,
                    v.avg_active_stake.round()
                );
                    index = index + 1;
                }
            }
        }

        // send the txs
        if !self.one_by_one {
            if added_validator_count != 0 || updated_validator_count != 0 || removed_count != 0 {
                println!("# sending transactions");
                marinade
                    .client
                    .process_transaction_sequence(common.simulate, builder.combined_sequence())?;
            }
        }
        Ok(())
    }
}
