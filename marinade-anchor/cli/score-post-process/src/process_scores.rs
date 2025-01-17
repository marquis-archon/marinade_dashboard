#![allow(unused_imports)]
use crate::Common;
use anyhow::bail;
use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::solana_sdk::{
    native_token::{lamports_to_sol, sol_to_lamports, LAMPORTS_PER_SOL},
    pubkey::Pubkey,
    signature::Signer,
    system_program,
};
use cli_common::{
    marinade_finance::{calc::proportional, state::StateHelpers},
    rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::{RpcMarinade, StakeInfo},
    transaction_builder::TransactionBuilder,
    Cluster,
};
use csv::*;
use log::{debug, error, info, warn};
use serde::Deserialize;

use std::io::{Read, Write};
use std::{collections::HashMap, str::FromStr};

use std::sync::Arc;
use structopt::StructOpt;

// deposit stake account control:
// we allow users to deposit stake accounts from validators with AT MOST 20% commission
const HEALTHY_VALIDATOR_MAX_COMMISSION: u8 = 20;
// Solana foundation do not stakes in validators if they're below 40% average
const MIN_AVERAGE_POSITION: f64 = 35.0;

#[derive(Debug, StructOpt)]
pub struct ProcessScoresOptions {
    #[structopt()]
    input_avg_file: Option<String>,

    #[structopt(
        long = "apy-file",
        help = "json APY file from stake-view.app to avoid adding low APY validators"
    )]
    apy_file: Option<String>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct ValidatorScoreRecord {
    rank: u32,
    pct: f64,
    epoch: u64,
    keybase_id: String,
    name: String,
    vote_address: String,
    score: u32,
    average_position: f64,
    commission: u8,
    epoch_credits: u64,
    data_center_concentration: f64,
    base_score: f64,
    mult: f64,
    avg_score: f64,
    avg_active_stake: f64,
    can_halt_the_network_group: bool,
    identity: String,
    stake_conc: f64,
}

// post-process data
#[derive(Debug, serde::Serialize)]
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

impl ValidatorScore {
    // we need all "healthy" validators in the on-chain list,
    // to enable "restricted_mode" deposit-stake-account (when auto_add_validator_enabled=false)
    // When auto_add_validator_enabled==false, you can only deposit stake-accounts
    // from validators already in the list, so we need to add all validators,
    // even those with with score==0, so people can deposit stake-accounts from those validators.
    // Having 0 score, the stake will be eventually moved to other validators
    /// Note: We only add validators in the on-chain list (allowing stake-account-deposits from those validators)
    /// when commission<HEALTHY_VALIDATOR_MAX_COMMISSION (30%)
    /// AND when average_position > 40 (50=average, 40=> at most 10% below average credits_observed)
    /// returns: 0=healthy, 1=warn (score *= 0.5), 2=unstake, 3=unstake & remove from list
    pub fn is_healthy(&self, avg_apy: f64, avg_this_epoch_credits: u64) -> (u8, String) {
        //
        // remove from concentrated validators
        if self.under_nakamoto_coefficient {
            return (
                2,
                format!(
                    "under Nakamoto coefficient. Staked:{} {}% of total",
                    self.avg_active_stake as u64, self.stake_concentration
                ),
            );
        } else if self.commission > HEALTHY_VALIDATOR_MAX_COMMISSION {
            return (3, format!("High commission {}%", self.commission));
        // Note: self.delinquent COMMENTED, a good validator could be delinquent for several minutes during an upgrade
        // it's better to consider this_epoch_credits as filter and not the on/off flag of self.delinquent
        // } else if self.delinquent {
        //     return (2, format!("DELINQUENT")); // keep delinquent validators in the list so people can escape by depositing stake accounts from them into Marinade
        } else if self.this_epoch_credits < avg_this_epoch_credits * 8 / 10 {
            return (
                2,
                format!(
                    "Very Low this_epoch_credits {}, average:{}, {}%",
                    self.this_epoch_credits,
                    avg_this_epoch_credits,
                    if avg_this_epoch_credits == 0 {
                        0
                    } else {
                        self.this_epoch_credits * 100 / avg_this_epoch_credits
                    }
                ),
            ); // keep delinquent validators in the list so people can escape by depositing stake accounts from them into Marinade
        } else if self.this_epoch_credits < avg_this_epoch_credits * 9 / 10 {
            return (
                1,
                format!(
                    "Low this_epoch_credits {}, average:{}, {}%",
                    self.this_epoch_credits,
                    avg_this_epoch_credits,
                    if avg_this_epoch_credits == 0 {
                        0
                    } else {
                        self.this_epoch_credits * 100 / avg_this_epoch_credits
                    }
                ),
            ); // keep delinquent validators in the list so people can escape by depositing stake accounts from them into Marinade
        } else if self.credits_observed == 0 {
            return (2, format!("ZERO CREDITS")); // keep them in the list so people can escape by depositing stake accounts from them into Marinade
        } else if self.apy.unwrap_or(6.0) < avg_apy / 2.0 {
            (2, format!("VERY Low APY {}%", self.apy.unwrap()))
        } else if self.apy.unwrap_or(6.0) < avg_apy * 0.80 {
            (1, format!("Low APY {}%", self.apy.unwrap()))
        } else if self.average_position < MIN_AVERAGE_POSITION {
            (1, format!("Low avg pos {}%", self.average_position))
            //
        } else {
            (0, "healthy".into())
        }
    }
}

impl ProcessScoresOptions {
    pub fn process(
        self,
        common: Common,
        client: Arc<RpcClient>,
        _cluster: Cluster,
    ) -> anyhow::Result<()> {
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

        let epoch_info = marinade.client.get_epoch_info()?;
        //
        //read file avg.csv into validator_scores:Vec
        let mut validator_scores: Vec<ValidatorScore> = Vec::with_capacity(2000);

        let input_avg_file = self.input_avg_file.unwrap_or("./avg.csv".into());
        {
            info!("Start from scores file {}", input_avg_file);
            {
                let mut validator_details_file_contents = String::new();
                let mut file = std::fs::File::open(&input_avg_file)?;
                file.read_to_string(&mut validator_details_file_contents)?;
                let mut reader =
                    csv::Reader::from_reader(validator_details_file_contents.as_bytes());
                for record in reader.deserialize() {
                    let record: ValidatorScoreRecord = record?;
                    validator_scores.push(ValidatorScore {
                        epoch: epoch_info.epoch,
                        rank: record.rank,
                        score: record.score,
                        name: record.name,
                        credits_observed: record.epoch_credits,
                        vote_address: record.vote_address,
                        commission: record.commission,
                        average_position: record.average_position,
                        data_center_concentration: record.data_center_concentration,
                        avg_active_stake: record.avg_active_stake,
                        apy: None,
                        delinquent: false,
                        this_epoch_credits: 0,
                        marinade_staked: 0.0,
                        pct: 0.0,
                        should_have: 0.0,
                        remove_level: 0,
                        remove_level_reason: String::from(""),
                        identity: record.identity,
                        keybase_id: record.keybase_id,
                        under_nakamoto_coefficient: record.can_halt_the_network_group,
                        stake_concentration: record.stake_conc,
                        base_score: record.base_score as u64,
                    });
                }
            }
        }
        // sort validator_scores by score desc
        validator_scores.sort_by(|a, b| b.score.cmp(&a.score));

        let mut total_score: u64 = validator_scores.iter().map(|s| s.score as u64).sum();
        info!(
            "avg file contains {} records, total_score {}",
            validator_scores.len(),
            total_score
        );

        let avg_this_epoch_credits: u64;
        let mut avg_apy: f64 = 5.0;
        const MIN_APY_TO_CONSIDER_FOR_AVG_APY: f64 = 4.0;

        // join other data from existing json files
        {
            // create a hashmap vote-key->index
            let validator_indices: HashMap<String, usize> = validator_scores
                .iter()
                .enumerate()
                .map(|(index, validator)| (validator.vote_address.to_string(), index))
                .collect();

            // get APY Data from stakeview.app
            // update "apy" field in validator_scores
            if let Some(apy_file) = self.apy_file {
                info!("Read APY from {}", apy_file);
                {
                    let file = std::fs::File::open(&apy_file)?;
                    let json_data: serde_json::Value = serde_json::from_reader(file)?;
                    let validators = &json_data["validators"];

                    let mut count_apy_data_points: usize = 0;
                    let mut sum_apy: f64 = 0.0;
                    match validators {
                        serde_json::Value::Array(list) => {
                            for apy_info in list {
                                if let Some(index) =
                                    validator_indices.get(apy_info["vote"].as_str().unwrap())
                                {
                                    let mut v = &mut validator_scores[*index];
                                    if let serde_json::Value::Number(x) = &apy_info["apy"] {
                                        let apy = x.as_f64().unwrap() * 100.0;
                                        if apy > MIN_APY_TO_CONSIDER_FOR_AVG_APY {
                                            count_apy_data_points += 1;
                                            sum_apy += apy;
                                        }
                                        v.apy = Some(apy);
                                    }
                                }
                            }
                        }
                        _ => panic!("invalid json"),
                    }
                    avg_apy = if count_apy_data_points == 0 {
                        4.5
                    } else {
                        sum_apy / count_apy_data_points as f64
                    };
                    info!("Avg APY {}", avg_apy);
                }
            }

            // get this_epoch_credits & delinquent Data from 'solana validators' output
            // update field in validator_scores
            {
                let mut count_credit_data_points: u64 = 0;
                let mut sum_this_epoch_credits: u64 = 0;
                let validators_file = "temp/solana-validators.json";
                info!("Read solana validators output from {}", validators_file);
                let file = std::fs::File::open(&validators_file)?;
                let json_data: serde_json::Value = serde_json::from_reader(file)?;
                let validators = &json_data["validators"];

                match validators {
                    serde_json::Value::Array(list) => {
                        for json_info in list {
                            if let Some(index) = validator_indices
                                .get(json_info["voteAccountPubkey"].as_str().unwrap())
                            {
                                let mut v = &mut validator_scores[*index];
                                if let serde_json::Value::Bool(x) = &json_info["delinquent"] {
                                    v.delinquent = *x
                                }
                                if let serde_json::Value::Number(x) = &json_info["epochCredits"] {
                                    let credits = x.as_u64().unwrap();
                                    v.this_epoch_credits = credits;
                                    sum_this_epoch_credits += credits;
                                    count_credit_data_points += 1;
                                }
                            }
                        }
                        avg_this_epoch_credits = sum_this_epoch_credits / count_credit_data_points;
                    }
                    _ => panic!("invalid json"),
                }
            }
        }

        let (current_validators, max_validators) = marinade.validator_list()?;
        info!(
            "Marinade on chain register: {} Validators of {} max capacity, total_score {}",
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
        info!(
            "Processing {} records in {} file",
            validator_scores.len(),
            input_avg_file.to_string()
        );

        // -----------
        // PASS 0 - set score = 0 if validator is not healthy (catch validators unhealthy now in this epoch)
        // -----------
        info!(
            "PASS 0 - set score = 0 if validator is not healthy (catch validators unhealthy now in this epoch)",
        );
        for v in validator_scores.iter_mut() {
            let (remove_level, reason) = v.is_healthy(avg_apy, avg_this_epoch_credits);
            v.remove_level = remove_level;
            v.remove_level_reason = reason;
            // if it is not healthy, adjust score to zero
            // score is computed based on last epoch, but APY & delinquent-status is current
            // so this will stop the bot staking on a validator that was very good last epochs
            // but delinquent on current epoch
            if remove_level == 1 {
                v.score /= 2
            } else if remove_level > 1 {
                v.score = 0
            };
            // BLACKLIST
            // manually slashed-paused
            // https://discord.com/channels/823564092379627520/856529851274887168/914462176205500446
            // Marinade is about to stake a validator that is intentionally delaying their votes to always vote in the correct fork. They changed the code so they don't waste any vote with consensus...
            // it seems like they are intentionally lagging their votes by about 50 slots or only voting on the fork with consensus, so that they don't vote on the wrong fork and so land every one of their votes... therefore their votes in effect don't contribute to the consensus of the network...
            // Response: slashing-pausing
            // 1) #14 Validator rep1xGEJzUiQCQgnYjNn76mFRpiPaZaKRwc13wm8mNr, score-pct:0.6037%
            // ValidatorScoreRecord { rank: 14, pct: 0.827161338644014, epoch: 252, keybase_id: "replicantstaking", name: "Replicant Staking", vote_address: "rep1xGEJzUiQCQgnYjNn76mFRpiPaZaKRwc13wm8mNr", score: 3211936, average_position: 57.8258431048359, commission: 0, epoch_credits: 364279, data_center_concentration: 0.03242, base_score: 363924.0, mult: 8.82584310483592, avg_score: 3211936.0, avg_active_stake: 6706.7905232706 }
            // avg-staked 6706.79, marinade-staked 50.13 (0.75%), should_have 39238.66, to balance +stake 39188.54 (accum +stake to this point 39188.54)
            if v.vote_address == "rep1xGEJzUiQCQgnYjNn76mFRpiPaZaKRwc13wm8mNr" {
                v.score = 0
            }
            // manually slashed-paused
            // Same entity 4block-team with 2 validators
            // https://discord.com/channels/823564092379627520/856529851274887168/916268033352302633
            // 4block-team case at 2021-12-3
            // current marinade stake: (4block-team validator#1)
            // 3) Validator 6anBvYWGwkkZPAaPF6BmzF6LUPfP2HFVhQUAWckKH9LZ, marinade-staked 55816.30 SOL, score-pct:0.7280%, 1 stake-accounts
            // next potential marinade stake: (4block-team validator#2)
            // 0) #6 0.72% m.stk:0 should:49761 next:+49761 credits:373961 cm:0 dcc:0.29698 4BLOCK.TEAM 2 - Now 0% Fees → 1% from Q1/2023 GfZybqTfVXiiF7yjwnqfwWKm2iwP96sSbHsGdSpwGucH
            if v.vote_address == "GfZybqTfVXiiF7yjwnqfwWKm2iwP96sSbHsGdSpwGucH" {
                v.score = 0
            }
            // Scrooge_McDuck
            // changing commission from 0% to 100% on epoch boundaries
            // https://www.validators.app/commission-changes?locale=en&network=mainnet
            if v.vote_address == "AxP8nEVvay26BvFqSVWFC73ciQ4wVtmhNjAkUz5szjCg" {
                v.score = 0
            }
            // Node Brothers
            // changing commission from 0% to 10% on epoch boundaries
            // https://www.validators.app/commission-changes/6895?locale=en&network=mainnet
            if v.vote_address == "DeFiDeAgFR29GgKdyyVZdvsELbDR8k4WqprWGtgtbi1o" {
                v.score = 0
            }
        }

        // -----------
        // PASS 1 - compute marinade staked & should_have
        // -----------
        info!("PASS 1 - compute marinade staked & should_have",);
        // imagine a +100K stake delta
        let total_stake_target = marinade
            .state
            .validator_system
            .total_active_balance
            .saturating_add(sol_to_lamports(100000.0));

        for v in validator_scores.iter_mut() {
            let vote = Pubkey::from_str(&v.vote_address)?;
            if let Some(_index) = validator_indices.get(&vote) {
                // get stakes
                let validator_stakes: Vec<&StakeInfo> = stakes
                    .iter()
                    .filter(|stake| {
                        if let Some(delegation) = stake.stake.delegation() {
                            // Only active stakes
                            delegation.deactivation_epoch == u64::MAX
                                && delegation.voter_pubkey == vote
                        } else {
                            false
                        }
                    })
                    .collect();
                let sum_stake = validator_stakes
                    .iter()
                    .map(|s| s.record.last_update_delegated_lamports)
                    .sum();

                // update on site, adjusted_score & sum_stake
                v.marinade_staked = lamports_to_sol(sum_stake);
                v.should_have = lamports_to_sol(
                    (v.score as f64 * total_stake_target as f64 / total_score as f64) as u64,
                );
            }
        }

        // adjust score
        // we use v.should_have as score
        for v in validator_scores.iter_mut() {
            // if we need to unstake, set a score that's x% of what's staked
            // so we ameliorate how aggressive the stake bot is for the 0-marinade-staked
            // unless this validator is marked for unstake
            v.score = if v.should_have < v.marinade_staked {
                // unstake
                if v.remove_level > 1 {
                    0
                } else if v.remove_level == 1 {
                    (v.marinade_staked * 0.5) as u32
                } else {
                    (v.marinade_staked * 0.90) as u32
                }
            } else {
                (v.should_have) as u32 // stake
            };
        }

        // recompute total score
        total_score = validator_scores.iter().map(|s| s.score as u64).sum();
        // sort validator_scores by score desc
        validator_scores.sort_by(|a, b| b.score.cmp(&a.score));
        // recompute should_have, rank and pct
        let mut rank: u32 = 1;
        for v in validator_scores.iter_mut() {
            v.should_have = lamports_to_sol(proportional(
                v.score as u64,
                total_stake_target,
                total_score,
            )?);
            v.rank = rank;
            rank += 1;
            // compute pct with 6 decimals precision
            v.pct = (v.score as u64 * 100_000_000 / total_score) as f64 / 1_000_000.0;
        }

        // ---------------------------
        // PASS 2 - save score as csv
        // ---------------------------
        info!("PASS 2 - save score as csv",);
        {
            let mut wtr = WriterBuilder::new()
                .flexible(true)
                .from_path("post-process.csv")?;
            let mut count = 0;
            for v in validator_scores {
                wtr.serialize(v)?;
                count += 1;
            }
            wtr.flush()?;
            info!("{} records", count);
        }
        Ok(())
    }
}
