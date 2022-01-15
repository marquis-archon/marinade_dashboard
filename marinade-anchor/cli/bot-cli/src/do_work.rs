use anyhow::Result;
use chrono::Duration;
use chrono::{offset::Utc, DateTime, Local, NaiveDateTime, TimeZone};
use cli_common::marinade_finance::state::StateHelpers;
use cli_common::solana_sdk::sysvar;
use cli_common::solana_sdk::{
    epoch_schedule::EpochSchedule,
    native_token::{lamports_to_sol, LAMPORTS_PER_SOL},
};
use cli_common::{
    instruction_helpers::InstructionHelpers, rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::RpcMarinade, transaction_builder::TransactionBuilder,
};
use log::info;
use std::ops::Sub;
use std::time::SystemTime;
use structopt::StructOpt;

const LAST_SLOTS_UNSAFE_MARGIN: u64 = 32; // It is too late and instruction can be processed in the next epoch

#[derive(StructOpt, Debug)]
pub struct DoWorkOptions {
    #[structopt(name = "max-run-minutes", default_value = "9", help = "max run time")]
    max_run_minutes: u16,
}

// fn do_work()
impl DoWorkOptions {
    pub fn process(
        self,
        common: &crate::Common,
        marinade: &mut RpcMarinade,
        builder: &mut TransactionBuilder,
    ) -> Result<()> {
        let start = SystemTime::now();
        //
        // get epoch information
        let clock = &marinade.get_clock()?;

        let epoch_schedule: EpochSchedule = bincode::deserialize(
            &marinade
                .client
                .get_account_data_retrying(&sysvar::epoch_schedule::ID)?,
        )?;
        let epoch_first_slot = epoch_schedule.get_first_slot_in_epoch(clock.epoch);
        let epoch_last_slot = epoch_schedule.get_last_slot_in_epoch(clock.epoch);
        let epoch_duration = epoch_last_slot.saturating_sub(epoch_first_slot) + 1;
        let epoch_slot = clock.slot.saturating_sub(epoch_first_slot);
        let advance: f64 = (epoch_slot * 100) as f64 / epoch_duration as f64;
        let ends_unix_timestamp = clock.epoch_start_timestamp
            + ((clock.unix_timestamp - clock.epoch_start_timestamp) as f64 * 100.0 / advance)
                as i64;
        let ends_datetime = NaiveDateTime::from_timestamp(ends_unix_timestamp, 0);
        let last_hour_utc = ends_datetime.sub(Duration::hours(1));
        let ends_local_datetime: DateTime<Local> = Local.from_utc_datetime(&ends_datetime);
        info!(
            "at epoch {} epoch-slot {} of {}, {}% advance, ends-UTC {:?}, ends local {:?}",
            clock.epoch,
            epoch_slot,
            epoch_duration,
            advance.round(),
            ends_datetime,
            ends_local_datetime
        );
        let last_hour = Utc::now().naive_utc() > last_hour_utc;
        if last_hour {
            info!("in LAST EPOCH HOUR");
        }
        let (current_validators, max_validators) = marinade.validator_list()?;
        info!(
            "Marinade on chain register: {} Validators of {} max capacity",
            current_validators.len(),
            max_validators
        );
        // create a hashmap PubKey->index
        let validators_with_score_count = current_validators
            .iter()
            .enumerate()
            .filter(|record| record.1.score > 0)
            .count() as u32;

        // -----------------
        // -- STAKE DELTA --
        // -----------------
        // check if we're in the stake-delta window (last part of the epoch)
        // and to play safe, also avoid doing things in LAST_SLOTS_UNSAFE_MARGIN
        let raw_reserve_balance_lamports = marinade
            .client
            .get_balance(&marinade.state.reserve_address())?;

        let total_stake_delta_i128: i128 = marinade.state.stake_delta(raw_reserve_balance_lamports);
        info!(
            "reserve_balance:{} SOL, stake_delta: {}, stake_delta_lamports i128:{}",
            lamports_to_sol(
                raw_reserve_balance_lamports
                    .saturating_sub(marinade.state.rent_exempt_for_token_acc)
            ),
            total_stake_delta_i128 as f64 / LAMPORTS_PER_SOL as f64,
            total_stake_delta_i128
        );
        let stake_delta_window = marinade.state.stake_system.slots_for_stake_delta;
        let stake_window_start_slot = epoch_last_slot.saturating_sub(stake_delta_window);
        info!(
            "stake-delta window:{} slots, starts at slot {}, current {}",
            stake_delta_window, stake_window_start_slot, clock.slot
        );
        if clock.slot > stake_window_start_slot
            && clock.slot < epoch_last_slot - LAST_SLOTS_UNSAFE_MARGIN
        {
            info!(
                "good window for stake-delta. {} mins remaining. Validators with score: {}",
                (epoch_last_slot - clock.slot) / 100,
                validators_with_score_count
            );
            // if zero, no stake needed
            if total_stake_delta_i128 == 0 {
                info!("--- but total_stake_delta_i128 == 0");
                // if positive but small, don't stake
            } else if total_stake_delta_i128 > 0
                && total_stake_delta_i128 < marinade.state.stake_system.min_stake as i128
            {
                info!(
                "--- not enough positive total_stake_delta_i128 {}, skipping stake-delta, min_stake: {}",
                lamports_to_sol(total_stake_delta_i128 as u64),
                lamports_to_sol(marinade.state.stake_system.min_stake)
            );
            } else
            // negative (unstake) or a positive (stake) significant amount
            {
                // ask for extra-runs
                if marinade.state.stake_system.extra_stake_delta_runs < validators_with_score_count
                    && clock.slot > stake_window_start_slot + stake_delta_window / 2
                {
                    builder.config_validator_system(
                        &marinade.state,
                        common.fee_payer.as_keypair(),
                        validators_with_score_count,
                    )?;
                    let result = marinade
                        .client
                        .process_transaction_sequence(common.simulate, builder.combined_sequence());
                    info!(
                        "asked for {} extra runs. Result:{:?}",
                        validators_with_score_count, result
                    )
                }

                info!("--- starting stake-delta");
                let stake_delta_options = crate::StakeDeltaOptions { rent_payer: None };
                let result = stake_delta_options.process(
                    &common,
                    marinade,
                    builder,
                    &start,
                    self.max_run_minutes as u32 * 60,
                ); // max run 9 minutes
                info!("stake-delta result: {:?}", result);
                // update state
                marinade.update()?;
            }
        } else
        // not in stake-delta window
        {
            info!(
                "*** waiting for the start of stake-delta window, in {} mins approx",
                stake_window_start_slot.saturating_sub(clock.slot) / 100
            );
        }

        // check if we're before the stake-delta window
        let before_the_stake_delta_window = clock.slot < stake_window_start_slot;
        info!(
            "before_the_stake_delta_window:{}",
            before_the_stake_delta_window
        );

        if before_the_stake_delta_window {
            // ------------------
            // -- UPDATE PRICE --
            // ------------------
            // do update_price only during the 1st half of the epoch
            if clock.slot < (epoch_last_slot + epoch_first_slot) / 2 {
                info!("--- starting update-price");
                let update_price_options = crate::UpdatePriceOptions {};
                let result = update_price_options.process(common, marinade, builder);
                info!("update-price result: {:?}", result);
                // update state
                marinade.update()?;
            }

            // --------------------------
            // -- MERGE STAKE-ACCOUNTS --
            // --------------------------
            let elapsed_secs = start.elapsed().unwrap().as_secs();
            let remaining_secs = (self.max_run_minutes as u64 * 60).saturating_sub(elapsed_secs);
            info!(
                "--- starting merge-stakes {} seconds remaining",
                remaining_secs
            );
            if remaining_secs > 10 {
                let result =
                    crate::merge_stakes::process(common, marinade, builder, remaining_secs);
                info!("merge-stakes result: {:?}", result);
            }
        } else {
            info!("end of do-work. We're in the stake-delta window.");
            info!("--- waiting for the next epoch to update-price and merge-accounts");
        }

        Ok(())
    }
}
