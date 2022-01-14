use std::{fs::File, io::Read, io::Write, path::PathBuf};

use cli_common::anchor_lang::AccountDeserialize;
use cli_common::anchor_spl::token::TokenAccount;

use cli_common::rpc_marinade::{RpcMarinade, StakeInfo};

use cli_common::marinade_finance::{
    calc::proportional,
    liq_pool::LiqPoolHelpers,
    located::Located,
    state::{State, StateHelpers},
    ticket_account::TicketAccountData,
};
use cli_common::solana_client::{
    rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType},
};
use cli_common::solana_sdk::{
    native_token::{lamports_to_sol, sol_to_lamports},
    pubkey::Pubkey,
    stake::program as stake_program,
    stake::state::StakeState,
};
use cli_common::spl_associated_token_account::get_associated_token_address;
use cli_common::spl_token;
use log::LevelFilter;

use cli_common::marinade_finance::validator_system::ValidatorRecord;

use structopt::StructOpt;

use crate::Command;

use crate::*;

// from cvs score file
#[allow(dead_code)]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default)]
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
}

#[allow(dead_code)]
#[derive(Debug, StructOpt)]
pub struct Show {
    #[structopt(
        short = "f",
        env = "FEE_PAYER",
        default_value = "~/.config/solana/id.json"
    )]
    fee_payer: InputKeypair,

    #[structopt(short = "u", help = "user account")]
    user_account: Option<InputPubkey>, //with non-zero stake

    #[structopt(short = "l", help = "list validators with non-zero stake")]
    list_validators: bool, //with non-zero stake

    #[structopt(
        long = "to-publish",
        help = "list validators also using score-file for publication"
    )]
    list_validators_to_publish: bool,

    #[structopt(
        long = "min-stake",
        help = "list validators with x millions min stake",
        default_value = "0"
    )]
    list_min_stake: f64, // list only above x

    #[structopt(
        long = "manual-unstake",
        help = "list validators candidates to manual-unstake"
    )]
    manual_unstake_candidates: bool,
    #[structopt(short = "a", help = "list all validators")]
    list_all_validators: bool,

    #[structopt(short = "r", help = "list raw stake-accounts from state")]
    list_raw_stake_accounts: bool,

    #[structopt(short = "w", help = "list stake-accounts by auth")]
    list_stake_by_auth: bool,

    #[structopt(short = "c", help = "list claimable ticket-accounts")]
    list_claims: bool,

    #[structopt(short = "t", help = "list all mSOL token accounts")]
    list_token_accounts: bool,

    #[structopt(short = "k", help = "show full information about a stake-account")]
    stake_account: Option<InputPubkey>,
}

impl Command for Show {
    fn process(self, common: Common, marinade: RpcMarinade) -> anyhow::Result<()> {
        // TODO: maybe move this calculation ...
        let verbose = common.verbose.get_level_filter(LevelFilter::Info) > LevelFilter::Info;
        debug!("State {:?}", marinade.state.as_ref());

        let epoch_info = marinade.client.get_epoch_info()?;
        info!("Epoch {:?}", epoch_info);

        const STAKE_WITHDRAW_SEED: &'static [u8] = b"withdraw";
        let stake_withdraw_auth = Pubkey::find_program_address(
            &[&marinade.state.key.to_bytes()[..32], STAKE_WITHDRAW_SEED],
            &cli_common::marinade_finance::ID,
        )
        .0;
        info!("Stake Withdraw Auth (PDA): {:?}", stake_withdraw_auth);

        if let Some(account) = self.stake_account {
            // get stake stake_account info
            let data = marinade.client.get_account_data(&account.as_pubkey())?;
            println!("{} data:{:?}", &account.as_pubkey(), data);
            let stake: StakeState = bincode::deserialize(&data)?;
            println!("{:?}", stake);
            return Ok(());
        }

        // show caps if enabled
        if marinade.state.staking_sol_cap < std::u64::MAX {
            info!(
                "Staking CAPPED TVL {} SOL",
                lamports_to_sol(marinade.state.staking_sol_cap)
            );
        }
        if marinade.state.liq_pool.liquidity_sol_cap < std::u64::MAX {
            info!(
                "Liquidity CAPPED TVL {} SOL",
                lamports_to_sol(marinade.state.liq_pool.liquidity_sol_cap)
            );
        }
        debug!(
            "slots_for_stake_delta {}, {} mins approx",
            marinade.state.stake_system.slots_for_stake_delta,
            marinade.state.stake_system.slots_for_stake_delta / 100
        );

        println!("-- Treasury ---------------");
        let reserve_address = marinade.state.reserve_address();
        let reserve_balance = marinade.client.get_balance(&reserve_address)?;

        println!(
            "reserve {} SOL (PDA) {}",
            lamports_to_sol(reserve_balance),
            reserve_address,
        );
        println!(
            "treasury mSOL account {} mSOL {}",
            token_balance_string(&marinade.client, &marinade.state.treasury_msol_account),
            marinade.state.treasury_msol_account
        );
        println!("-- Config ---------------");
        println!(
            "rent_exempt_for_token_acc {}",
            marinade.state.rent_exempt_for_token_acc
        );
        println!(
            "min_deposit {} SOL",
            lamports_to_sol(marinade.state.min_deposit)
        );
        println!(
            "min_stake {} SOL",
            lamports_to_sol(marinade.state.stake_system.min_stake)
        );
        println!("reward_fee {}", marinade.state.reward_fee);

        println!(
            "mSOL supply {}",
            lamports_to_sol(marinade.state.total_virtual_staked_lamports())
        );

        println!("-- mSOL token ---------------");
        println!(
            "mSOL price {} SOL (start epoch price {} SOL)",
            lamports_to_sol(marinade.state.calc_lamports_from_msol_amount(
                sol_to_lamports(1.0),
                // msol_mint_state.supply
            )?,),
            marinade.state.msol_price as f64 / State::PRICE_DENOMINATOR as f64
        );
        println!(
            "mSOL supply {} mint {} auth {}",
            lamports_to_sol(marinade.state.msol_supply),
            marinade.state.msol_mint,
            marinade.state.msol_mint_authority()
        );

        println!("-- Liq-Pool ---------------");
        println!(
            "mSOL-SOL-LP supply {} mint {} auth {}",
            lamports_to_sol(marinade.state.liq_pool.lp_supply),
            marinade.state.as_ref().liq_pool.lp_mint,
            marinade.state.lp_mint_authority()
        );

        let msol_leg = marinade
            .client
            .get_token_account(&marinade.state.liq_pool.msol_leg)
            .unwrap()
            .unwrap();

        let msol_authority = marinade.state.liq_pool_msol_leg_authority();
        println!(
            "mSOL  {} account {} auth {}",
            msol_leg.token_amount.real_number_string(),
            marinade.state.liq_pool.msol_leg,
            msol_authority,
        );
        if verbose {
            println!("      mint:{} owner:{}", msol_leg.mint, msol_leg.owner);
        }

        let sol_leg_pubkey = marinade.state.liq_pool_sol_leg_address();
        let sol_leg_account = marinade.client.get_account(&sol_leg_pubkey)?;
        println!(
            "SOL   {} account {} ", //native PDA account, program is authority
            lamports_to_sol(sol_leg_account.lamports),
            sol_leg_pubkey
        );
        println!(
            "Liquidity Target: {}",
            lamports_to_sol(marinade.state.liq_pool.lp_liquidity_target)
        );
        println!(
            "Current-fee: {}",
            marinade.state.liq_pool.linear_fee(sol_leg_account.lamports)
        );
        println!(
            "Min-Max-Fee: {}-{}",
            marinade.state.liq_pool.lp_min_fee, marinade.state.liq_pool.lp_max_fee
        );
        println!("Treasury cut: {}", marinade.state.liq_pool.treasury_cut);
        println!("--------------------------");
        println!("reserve balance: {}", lamports_to_sol(reserve_balance));
        println!(
            "cooling down: {}",
            lamports_to_sol(marinade.state.stake_system.delayed_unstake_cooling_down)
        );
        println!(
            "Circulating ticket accounts: {} ({} tickets)",
            lamports_to_sol(marinade.state.circulating_ticket_balance),
            marinade.state.circulating_ticket_count,
        );
        let stake_delta: i128 = marinade.state.stake_delta(reserve_balance);
        println!(
            "stake-delta: {}",
            stake_delta.signum() as f64 * lamports_to_sol(stake_delta.abs() as u64)
        );

        let user_account = if self.user_account.is_none() {
            self.fee_payer.as_pubkey()
        } else {
            self.user_account.unwrap().as_pubkey()
        };

        // to-publish mode
        let to_publish_mode = self.list_validators_to_publish
            || self.manual_unstake_candidates
            || self.list_min_stake > 0.0;

        if !self.list_validators && !to_publish_mode {
            println!("--------------------------");
            println!("-- Your Token Accounts, {}---", user_account);

            show_token_balance(
                &user_account,
                &marinade.state.msol_mint,
                "mSOL",
                marinade.client.clone(),
                verbose,
            );

            show_token_balance(
                &user_account,
                &marinade.state.liq_pool.lp_mint,
                "mSOL-SOL-LP",
                marinade.client.clone(),
                verbose,
            );

            println!("--------------------------");
        }

        //claim accounts
        //if self.list_claims || verbose {
        const TICKET_ACCOUNT_SPACE: usize = 8 + std::mem::size_of::<TicketAccountData>();
        use cli_common::anchor_lang::Discriminator;
        let ticket_filter_data: Vec<u8> = [
            &TicketAccountData::discriminator()[..],
            &common.instance.as_pubkey().to_bytes(),
            &user_account.to_bytes(),
        ]
        .concat();
        //ask the RPC server for all user's ticket accounts
        let list = marinade
            .client
            .get_program_accounts_with_config(
                &cli_common::marinade_finance::ID,
                RpcProgramAccountsConfig {
                    filters: Some(vec![
                        RpcFilterType::DataSize(TICKET_ACCOUNT_SPACE as u64),
                        RpcFilterType::Memcmp(Memcmp {
                            offset: 0,
                            bytes: MemcmpEncodedBytes::Binary(
                                bs58::encode(&ticket_filter_data).into_string(),
                            ),
                            encoding: None,
                        }),
                    ]),
                    account_config: RpcAccountInfoConfig {
                        encoding: None,
                        commitment: Some(marinade.client.commitment()),
                        ..RpcAccountInfoConfig::default()
                    },
                    with_context: None,
                },
            )
            .unwrap();
        //print tickets
        if !list.is_empty() {
            println!("--------------------------");
            println!("-- Your claim tickets {} ---", user_account);
            for i in list {
                let ticket_data: TicketAccountData =
                    AccountDeserialize::try_deserialize(&mut i.1.data.as_slice())?;
                println!(
                    "-- {} SOL ticket {}, epoch-created:{}",
                    lamports_to_sol(ticket_data.lamports_amount),
                    i.0,
                    ticket_data.created_epoch
                );
            }
            println!(
                "-- current epoch:{} advance:{}%",
                epoch_info.epoch,
                proportional(epoch_info.slot_index, 100, epoch_info.slots_in_epoch).unwrap()
            );
            println!("--------------------------");
        }
        //}

        if self.list_raw_stake_accounts {
            let (stakes, max_stakes) = marinade.stake_list()?;
            println!("--------------------------");
            println!(
                "Stake list account: {} with {}/{} stakes",
                marinade.state.stake_system.stake_list_address(),
                stakes.len(),
                max_stakes
            );

            //----------
            // only pubkey, sorted
            // stakes.sort_by(|a, b| b.stake_account.cmp(&a.stake_account));
            // for stake in stakes.iter().enumerate() {
            //     println!("{}", stake.stake_account);
            // }
            //----------
            //only pubkey, with index
            //for (index, stake) in stakes.iter().enumerate() {
            //    println!("{} {:?}", index + 1, stake);
            //}
            //----------
            // full data for each account
            let mut acc_detail_csv = vec![];
            acc_detail_csv.push("account,stake,activation_epoch,deactivation_epoch,voter_pubkey,credits_observed,record_last_updated_epoch, record_last_update_delegated_lamports".into());
            for (index, record) in stakes.iter().enumerate() {
                println!("{}) stake {:?}", index + 1, record);
                if let Ok(acc_data) = marinade.client.get_account_data(&record.stake_account) {
                    let stake_info: StakeState = bincode::deserialize(&acc_data)?;
                    println!("-- {:?}", stake_info);
                    let data = stake_info.stake().unwrap();
                    acc_detail_csv.push(format!(
                        r#""{}",{},{},{},"{}",{},{},{}"#,
                        record.stake_account,
                        data.delegation.stake,
                        data.delegation.activation_epoch,
                        data.delegation.deactivation_epoch,
                        data.delegation.voter_pubkey,
                        data.credits_observed,
                        record.last_update_epoch,
                        record.last_update_delegated_lamports
                    ))
                } else {
                    println!("Err acc not found {:?}", record);
                }
            }
            let filename = PathBuf::default()
                .join("data")
                .join(format!("{}-marinade-stake-accounts.csv", epoch_info.epoch));
            info!("Writing {}", filename.to_str().unwrap());
            let mut file = File::create(filename)?;
            file.write_all(&acc_detail_csv.join("\n").into_bytes())?;
            println!("--------------------------");
        }

        //token accounts by mint
        if self.list_token_accounts {
            println!("----token accounts by mint");
            //ask the RPC server for all our stake accounts
            let mint = Pubkey::from_str("mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So").unwrap();
            let cfg = RpcProgramAccountsConfig {
                account_config: RpcAccountInfoConfig {
                    encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
                    commitment: Some(marinade.client.commitment()),
                    ..RpcAccountInfoConfig::default()
                },
                filters: Some(vec![
                    RpcFilterType::DataSize(TokenAccount::LEN as u64),
                    RpcFilterType::Memcmp(Memcmp {
                        offset: 0,
                        bytes: MemcmpEncodedBytes::Binary(mint.to_string()),
                        encoding: None,
                    }),
                ]),
                with_context: None,
            };
            //println!("{}",json!([&spl_token::ID.to_string(), cfg]));
            let list = marinade
                .client
                .get_program_accounts_with_config(&spl_token::ID, cfg)
                .unwrap();
            if !list.is_empty() {
                println!("--------------------------");
                println!("-- {} Token accounts by mint {}", list.len(), mint);
                // unpack to later sort by amount
                let mut parsed: Vec<(Pubkey, Pubkey, u64)> = list
                    .into_iter()
                    .map(|i| {
                        //let info = SPLTokenAccount::unpack_from_slice(&i.1.data.as_slice()).unwrap();
                        let info = TokenAccount::try_deserialize(&mut i.1.data.as_slice()).unwrap();
                        (i.0, info.owner, info.amount)
                    })
                    .collect();
                // sort by amount asc
                parsed.sort_by(|a, b| a.2.cmp(&b.2));
                // for i in list {
                //     println!("{} {:?}", i.0, i.1);
                // }
                println!("--------------------------");
                let filename = PathBuf::default()
                    .join("data")
                    .join(format!("{}-mSOL-token-holders.csv", epoch_info.epoch));
                info!("Writing {}", filename.to_str().unwrap());
                let mut file = File::create(&filename)?;
                file.write("account,amount\n".as_bytes())?;
                for i in parsed {
                    println!("{} {}", i.0, lamports_to_sol(i.2));
                    file.write(format!("\"{}\",{}\n", i.0, lamports_to_sol(i.2)).as_bytes())?;
                }
                println!("--------------------------");
                info!("saved as {}", &filename.to_str().unwrap());
            }
        }

        //stake accounts by authority
        if self.list_stake_by_auth {
            println!("----stake accounts by authority");
            //ask the RPC server for all our stake accounts
            const STAKE_ACCOUNT_SPACE: usize = std::mem::size_of::<StakeState>();
            let filter_data = stake_withdraw_auth.to_bytes();
            let mut list = marinade
                .client
                .get_program_accounts_with_config(
                    &stake_program::ID,
                    RpcProgramAccountsConfig {
                        filters: Some(vec![
                            RpcFilterType::DataSize(STAKE_ACCOUNT_SPACE as u64),
                            RpcFilterType::Memcmp(Memcmp {
                                offset: 44,
                                bytes: MemcmpEncodedBytes::Binary(
                                    bs58::encode(&filter_data).into_string(),
                                ),
                                encoding: None,
                            }),
                        ]),
                        account_config: RpcAccountInfoConfig {
                            encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
                            commitment: Some(marinade.client.commitment()),
                            ..RpcAccountInfoConfig::default()
                        },
                        with_context: None,
                    },
                )
                .unwrap();
            if !list.is_empty() {
                println!("--------------------------");
                println!("-- {} stake accounts by staker auth ---", list.len());
                list.sort_by(|a, b| b.0.cmp(&a.0));
                // for i in list {
                //     println!("{}", i.0);
                // }
                for i in list {
                    let stake_info: StakeState = bincode::deserialize(&i.1.data.as_slice())?;
                    println!("{} {:?}", i.0, stake_info);
                }
                println!("--------------------------");
            }
        }

        if self.list_validators {
            let (validators, max_validators) = marinade.validator_list()?;
            let (stakes, max_stakes) = marinade.stakes_info()?;
            println!(
                "validator_manager_authority {}",
                marinade.state.validator_system.manager_authority
            );
            println!(
                "Stake list account: {} with {}/{} stakes",
                marinade.state.stake_system.stake_list_address(),
                stakes.len(),
                max_stakes
            );

            let mut total_staked: u64 = 0;
            let mut total_staked_fully_activated: u64 = 0;

            println!("-----------------");
            println!("-- Validators ---");
            println!(
                "Total staked: {} SOL",
                lamports_to_sol(marinade.state.validator_system.total_active_balance),
            );
            println!(
                "List account: {} with {}/{} validators",
                marinade.state.validator_system.validator_list_address(),
                validators.len(),
                max_validators
            );

            let mut count_staked: u16 = 0;
            let mut count_staked_and_active: u16 = 0;
            for (index, validator) in validators.iter().enumerate() {
                let validator_stakes: Vec<&StakeInfo> = stakes
                    .iter()
                    .filter(|stake| {
                        if let Some(delegation) = stake.stake.delegation() {
                            // Only active stakes
                            delegation.deactivation_epoch == u64::MAX
                                && delegation.voter_pubkey == validator.validator_account
                        } else {
                            false
                        }
                    })
                    .collect();

                if !self.list_all_validators && validator.active_balance == 0 {
                    continue;
                }

                count_staked += 1;
                println!("-------------------------------------------------------------");
                println!(
                    "{}) Validator {}, marinade-staked {:.2} SOL, score-pct:{:.4}%, {} stake-accounts",
                    index + 1,
                    validator.validator_account,
                    lamports_to_sol(validator.active_balance),
                    //validator.score,marinade.state.validator_system.total_validator_score,
                    validator.score as f32 * 100.0
                        / marinade.state.validator_system.total_validator_score as f32,
                    validator_stakes.len()
                );

                let mut validator_has_active_stake = false;
                for stake in validator_stakes {
                    let delegation = stake.stake.delegation().unwrap();
                    print!(
                        "  {}. Stake {} delegated {} activation_epoch:{}",
                        stake.index,
                        stake.record.stake_account,
                        lamports_to_sol(delegation.stake),
                        delegation.activation_epoch
                    );
                    total_staked += delegation.stake;
                    if delegation.activation_epoch < epoch_info.epoch - 1 {
                        total_staked_fully_activated += delegation.stake;
                        validator_has_active_stake = true;
                    }
                    let extra_balance = lamports_to_sol(
                        stake.balance
                            - delegation.stake
                            - stake.stake.meta().unwrap().rent_exempt_reserve,
                    );
                    if extra_balance > 0.0 {
                        print!(" (extra balance {})", extra_balance);
                    }
                    println!();
                }
                if validator_has_active_stake {
                    count_staked_and_active += 1;
                }
            }
            println!("--------------------------");
            println!(
                " {} validators listed, {} with active stakes, total_staked {} total_staked_fully_activated {}, warming-up in this epoch:{}",
                count_staked,
                count_staked_and_active,
                lamports_to_sol(total_staked),
                lamports_to_sol(total_staked_fully_activated),
                lamports_to_sol(total_staked.saturating_sub(total_staked_fully_activated)),
            );
            // save {cluster}-validator-detail.csv (repeating the cluster in the name is intentional)
            // let filename = PathBuf::from_str("data")?.join("marinade_staking.csv");
            // info!("Writing {}", filename.display());
            // let mut file = File::create(filename)?;
            // file.write_all(&report_lines.join("\n").into_bytes())?;

            let cooling_down_stakes: Vec<&StakeInfo> = stakes
                .iter()
                .filter(|stake| {
                    if let Some(delegation) = stake.stake.delegation() {
                        delegation.deactivation_epoch != u64::MAX
                    } else {
                        true
                    }
                })
                .collect();
            if cooling_down_stakes.len() > 0 {
                println!("--------------------------");
                println!("-- Cooling down stakes ---");
                for stake in cooling_down_stakes {
                    if let Some(delegation) = stake.stake.delegation() {
                        let extra_balance = lamports_to_sol(
                            stake.balance
                                - delegation.stake
                                - stake.stake.meta().unwrap().rent_exempt_reserve,
                        );
                        print!(
                            "  {}. Stake {} delegated {} to {}",
                            stake.index,
                            stake.record.stake_account,
                            lamports_to_sol(delegation.stake),
                            delegation.voter_pubkey,
                        );
                        if extra_balance > 0.0 {
                            print!(" (extra-balance {})", extra_balance)
                        }
                        println!();
                    } else {
                        println!(
                            "  {}. Stake {} (full balance {})",
                            stake.index,
                            stake.record.stake_account,
                            lamports_to_sol(stake.balance)
                        )
                    }
                }
            }
        }

        if to_publish_mode {
            let (mut validators, max_validators) = marinade.validator_list()?;
            println!(
                "validator_manager_authority {}",
                marinade.state.validator_system.manager_authority
            );

            let mut validator_scores: Vec<ValidatorScoreRecord> = Vec::with_capacity(2000);
            //------------------------------------------
            {
                //------------------------------------------
                // try to get validator classification
                // TODO optional, file as param
                let scores_file = "./avg.csv";
                info!("reading scores from {}", scores_file);
                //read file validator_details.csv into validator_scores:Vec
                {
                    let mut validator_details_file_contents = String::new();
                    let mut file = std::fs::File::open(scores_file)?;
                    file.read_to_string(&mut validator_details_file_contents)?;
                    let mut reader =
                        csv::Reader::from_reader(validator_details_file_contents.as_bytes());
                    for record in reader.deserialize() {
                        let record: ValidatorScoreRecord = record?;
                        validator_scores.push(record);
                    }
                }
            }
            println!("-----------------");
            println!("-- Validators ---");
            println!(
                "Total staked: {} SOL",
                lamports_to_sol(marinade.state.validator_system.total_active_balance),
            );
            println!(
                "List account: {} with {}/{} validators",
                marinade.state.validator_system.validator_list_address(),
                validators.len(),
                max_validators
            );

            let mut count_staked: u16 = 0;
            // sort validator_scores by score desc (for stake)
            // and after that, by unbalance for unstake
            validators.sort_by_key(|a| {
                let should = should_have(
                    &a,
                    marinade.state.validator_system.total_active_balance,
                    marinade.state.validator_system.total_validator_score,
                );
                if should > 0 && should > a.active_balance {
                    -(a.score as i64)
                } else {
                    (a.active_balance - should) as i64
                }
            });

            // // sort validator_scores by MOST UNBALANCED
            // validators.sort_by(|a, b| {
            //     unbalance(
            //         &b,
            //         marinade.state.validator_system.total_active_balance,
            //         marinade.state.validator_system.total_validator_score,
            //     )
            //     .cmp(&unbalance(
            //         &a,
            //         marinade.state.validator_system.total_active_balance,
            //         marinade.state.validator_system.total_validator_score,
            //     ))
            // });
            println!(
                "-----------------------------------------------------------------------------"
            );
            println!("-- SORTED by #Rank, first the ones requiring stake, then the ones requiring unstake");
            if self.manual_unstake_candidates {
                println!("-- searching for manual-unstake candidates");
            }
            println!(
                "-----------------------------------------------------------------------------"
            );

            let mut accum_plus_stake: u64 = 0;
            let mut accumulated_manual_unstakes: f64 = 0.0;
            for (index, validator) in validators.iter().enumerate() {
                // get latest scoring info
                let empty_record = ValidatorScoreRecord::default();
                let searched = if to_publish_mode {
                    validator_scores.iter().find(|v| {
                        Pubkey::from_str(&v.vote_address).unwrap() == validator.validator_account
                    })
                } else {
                    None
                };
                let record = if searched.is_some() {
                    searched.unwrap()
                } else {
                    &empty_record
                };

                if self.list_min_stake > 0.0
                    && record.avg_active_stake < self.list_min_stake * 1_000_000.0
                {
                    continue;
                }

                let should_have = (validator.score as u128
                    * marinade.state.validator_system.total_active_balance as u128
                    / marinade.state.validator_system.total_validator_score as u128)
                    as u64;
                let next_operation = should_have as i64 - validator.active_balance as i64;
                let marinade_pct =
                    lamports_to_sol(validator.active_balance) / record.avg_active_stake * 100.0;

                // if we're looking for manual_unstake_candidates
                let include_in_list = if self.manual_unstake_candidates {
                    // let's see which ones we skip
                    if marinade_pct == 0.0 {
                        // we don't have stake there
                        false
                    } else if record.rank < 250 {
                        // good rank
                        false
                    } else if lamports_to_sol(validator.active_balance) < 5.0 {
                        // our stake there is too low to even bother
                        false
                    } else if record.average_position <= 20.0 {
                        // this one we include (low performance)
                        true
                    } else if record.avg_active_stake > 3_500_000.0 {
                        // concentrated validator
                        true
                    } else if record.average_position >= 47.0 {
                        // this one we don't (not so bad performance)
                        false
                    } else if marinade_pct > 50.0 {
                        // mercy rule
                        false
                    } else {
                        true
                    }
                } else {
                    true
                };
                if !include_in_list {
                    continue;
                }

                println!("-------------------------------------------------------------");
                println!(
                    "{}) #{} Validator {}, score-pct:{:.4}%",
                    index + 1,
                    record.rank,
                    validator.validator_account,
                    //validator.score,marinade.state.validator_system.total_validator_score,
                    validator.score as f32 * 100.0
                        / marinade.state.validator_system.total_validator_score as f32
                );
                if validator.active_balance > 0 {
                    count_staked += 1;
                    accumulated_manual_unstakes += lamports_to_sol(validator.active_balance);
                }
                if to_publish_mode {
                    println!("{:?}", record);
                    if record.average_position < 50.0 {
                        println!("-- *** LOW AVG POSITION {}", record.average_position);
                    }
                    if record.epoch_credits < 300000 {
                        println!(
                            "-- *** LOW record.credits_observed {}",
                            record.epoch_credits
                        );
                    }
                    if record.commission > 10 {
                        println!("-- *** HIGH COMMISSION {}", record.commission);
                    }
                    // if record.can_halt_the_network_group {
                    //     println!(
                    //         "-- *** BELOW NAKAMOTO COEFFICIENT, concentrated stake {} million SOL",
                    //         record.active_stake / 1_000_000.0
                    //     );
                    // }
                }

                print!(
                    " avg-staked {:.2}, marinade-staked {:.2} ({:.2}%), should_have {:.2}, ",
                    record.avg_active_stake,
                    lamports_to_sol(validator.active_balance),
                    marinade_pct,
                    lamports_to_sol(should_have)
                );
                if next_operation > 0 {
                    print!(
                        "to balance +stake {:.2}",
                        lamports_to_sol(next_operation as u64)
                    );
                    accum_plus_stake += next_operation as u64
                } else if next_operation == 0 {
                    print!("balanced");
                    accum_plus_stake += next_operation as u64
                } else {
                    print!(
                        "to balance -unstake {:.2}",
                        lamports_to_sol(-next_operation as u64)
                    );
                }
                if next_operation > 0 {
                    print!(
                        " (accum +stake to this point {:.2})",
                        lamports_to_sol(accum_plus_stake)
                    );
                }
                println!();
                if self.manual_unstake_candidates {
                    print!(
                        "./validator-manager emergency-unstake {}",
                        validator.validator_account
                    );
                }
                println!();
            }
            println!("--------------------------");
            println!(" {} validators with stake", count_staked);

            if self.manual_unstake_candidates {
                println!(
                    " {} accumulated_manual_unstakes",
                    accumulated_manual_unstakes.round()
                );
            }

            // save {cluster}-validator-detail.csv (repeating the cluster in the name is intentional)
            /*
            let filename = PathBuf::from_str("data")?.join("marinade_staking.csv");
            info!("Writing {}", filename.display());
            let mut file = File::create(filename)?;
            file.write_all(&report_lines.join("\n").into_bytes())?;
            */
        }

        println!("--");
        Ok(())
    }
}

pub fn unbalance(
    v: &ValidatorRecord,
    total_active_balance: u64,
    total_validator_score: u32,
) -> i128 {
    let should_have =
        (v.score as u128 * total_active_balance as u128 / total_validator_score as u128) as u64;
    return should_have as i128 - v.active_balance as i128;
}

pub fn should_have(
    v: &ValidatorRecord,
    total_active_balance: u64,
    total_validator_score: u32,
) -> u64 {
    (v.score as u128 * total_active_balance as u128 / total_validator_score as u128) as u64
}

fn token_balance(client: impl AsRef<RpcClient>, token_account_pubkey: &Pubkey) -> Option<String> {
    let token_account_get_balance_result = client
        .as_ref()
        .get_token_account_balance(token_account_pubkey);
    if let Ok(ui_token_amount) = token_account_get_balance_result {
        Some(ui_token_amount.real_number_string_trimmed())
    } else {
        None
    }
}

fn token_balance_string(client: impl AsRef<RpcClient>, token_account_pubkey: &Pubkey) -> String {
    token_balance(client, token_account_pubkey).unwrap_or_else(|| "0".to_string())
}

fn show_token_balance(
    owner: &Pubkey,
    mint: &Pubkey,
    symbol: &str,
    client: impl AsRef<RpcClient> + Clone,
    verbose: bool,
) {
    let token_account_pubkey = get_associated_token_address(&owner, &mint);

    let token_balance_string = token_balance_string(client.clone(), &token_account_pubkey);
    println!(
        "-- {:10} balance {} addr {:?} ",
        symbol, token_balance_string, token_account_pubkey,
    );

    if token_balance_string != *"0" && verbose {
        let token_account_data = client
            .as_ref()
            .get_token_account(&token_account_pubkey)
            .unwrap()
            .unwrap();
        println!(
            "--            mint {} owner {} delegate {:?}",
            token_account_data.mint, token_account_data.owner, token_account_data.delegate
        );
    }
}
