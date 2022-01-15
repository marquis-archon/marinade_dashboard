use std::str::FromStr;

use crate::error::Result;
use anyhow::bail;
use chrono::Duration;
use marstat::{BalancesSample, Farms};
use solana_sdk::{native_token::lamports_to_sol, pubkey::Pubkey};

use crate::{apy::calc_apy, MarstatConnection};

pub struct MarinadeMetrics {
    pub msol_price: f64,
    pub msol_apy_7d: f64,
    pub msol_apy_30d: f64,
    pub msol_apy_365d: f64,

    pub lp_price: f64,
    pub lp_apy_1d: f64,
    pub lp_apy_7d: f64,
    pub lp_apy_30d: f64,
    pub lp_apy_365d: f64,

    pub msol_farm_total_staked: f64,
    pub msol_farm_total_rewards_per_week: f64,
    pub msol_farm_rewards_per_token: f64,

    pub lp_farm_total_staked: f64,
    pub lp_farm_total_rewards_per_week: f64,
    pub lp_farm_rewards_per_token: f64,
}

pub fn get_metrics(connection: &mut postgres::Client) -> anyhow::Result<MarinadeMetrics> {
    let current_sample = BalancesSample::db_read_last(connection)?;

    let sample_1d_before = BalancesSample::db_read_last_before_or_first(
        connection,
        current_sample.slot_time - Duration::days(1),
    )?;
    if sample_1d_before.slot >= current_sample.slot {
        bail!("Not enough data");
    }

    let sample_7d_before = BalancesSample::db_read_last_before_or_first(
        connection,
        current_sample.slot_time - Duration::weeks(1),
    )?;
    if sample_7d_before.slot >= current_sample.slot {
        bail!("Not enough data");
    }

    let sample_30d_before = BalancesSample::db_read_last_before_or_first(
        connection,
        current_sample.slot_time - Duration::days(30),
    )?;
    if sample_30d_before.slot >= current_sample.slot {
        bail!("Not enough data");
    }

    let sample_365d_before = BalancesSample::db_read_last_before_or_first(
        connection,
        current_sample.slot_time - Duration::days(365),
    )?;
    if sample_365d_before.slot >= current_sample.slot {
        bail!("Not enough data");
    }

    let msol_price = current_sample.msol_price();
    let lp_price = current_sample.lp_price();

    let msol_farm = Farms::db_read_last(
        connection,
        &Pubkey::from_str("mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So").unwrap(),
    )?;

    let msol_farm_begin = Farms::db_read_first(
        connection,
        &Pubkey::from_str("mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So").unwrap(),
    )?
    .unwrap();

    let lp_farm = Farms::db_read_last(
        connection,
        &Pubkey::from_str("LPmSozJJ8Jh69ut2WP3XmVohTjL4ipR18yiCzxrUmVj").unwrap(),
    )?;

    let lp_farm_begin = Farms::db_read_first(
        connection,
        &Pubkey::from_str("LPmSozJJ8Jh69ut2WP3XmVohTjL4ipR18yiCzxrUmVj").unwrap(),
    )?
    .unwrap();

    Ok(MarinadeMetrics {
        msol_price,
        msol_apy_7d: calc_apy(
            current_sample.slot_time,
            msol_price,
            sample_7d_before.slot_time,
            sample_7d_before.msol_price(),
        ),
        msol_apy_30d: calc_apy(
            current_sample.slot_time,
            msol_price,
            sample_30d_before.slot_time,
            sample_30d_before.msol_price(),
        ),
        msol_apy_365d: calc_apy(
            current_sample.slot_time,
            msol_price,
            sample_365d_before.slot_time,
            sample_365d_before.msol_price(),
        ),
        lp_price,
        lp_apy_1d: calc_apy(
            current_sample.slot_time,
            lp_price,
            sample_1d_before.slot_time,
            sample_1d_before.lp_price(),
        ),
        lp_apy_7d: calc_apy(
            current_sample.slot_time,
            lp_price,
            sample_7d_before.slot_time,
            sample_7d_before.lp_price(),
        ),
        lp_apy_30d: calc_apy(
            current_sample.slot_time,
            lp_price,
            sample_30d_before.slot_time,
            sample_30d_before.lp_price(),
        ),
        lp_apy_365d: calc_apy(
            current_sample.slot_time,
            lp_price,
            sample_365d_before.slot_time,
            sample_365d_before.lp_price(),
        ),

        msol_farm_total_staked: lamports_to_sol(msol_farm.total_tokens_deposited),
        msol_farm_total_rewards_per_week: lamports_to_sol(msol_farm.annual_rewards_rate) * 7.0
            / 365.0,
        msol_farm_rewards_per_token: msol_farm.rewards_per_token_stored
            - msol_farm_begin.rewards_per_token_stored,

        lp_farm_total_staked: lamports_to_sol(lp_farm.total_tokens_deposited),
        lp_farm_total_rewards_per_week: lamports_to_sol(lp_farm.annual_rewards_rate) * 7.0 / 365.0,
        lp_farm_rewards_per_token: lp_farm.rewards_per_token_stored
            - lp_farm_begin.rewards_per_token_stored,
    })
}

#[get("/metrics")]
pub async fn metrics(connection: MarstatConnection) -> Result<String> {
    let MarinadeMetrics {
        msol_price,
        msol_apy_7d,
        msol_apy_30d,
        msol_apy_365d,
        lp_price,
        lp_apy_1d,
        lp_apy_7d,
        lp_apy_30d,
        lp_apy_365d,
        msol_farm_total_staked,
        msol_farm_total_rewards_per_week,
        msol_farm_rewards_per_token,
        lp_farm_total_staked,
        lp_farm_total_rewards_per_week,
        lp_farm_rewards_per_token,
    } = connection.run(move |c| get_metrics(c)).await?;

    Ok(format!(
        "marinade_msol_price {}\n\
         marinade_msol_apy_7d {}\n\
         marinade_msol_apy_30d {}\n\
         marinade_msol_apy_365d {}\n\
         marinade_lp_price {}\n\
         marinade_lp_apy_1d {}\n\
         marinade_lp_apy_7d {}\n\
         marinade_lp_apy_30d {}\n\
         marinade_lp_apy_365d {}\n\
         marinade_msol_farm_total_staked {}\n\
         marinade_msol_farm_total_rewards_per_week {}\n\
         marinade_msol_farm_rewards_per_token {}\n\
         marinade_lp_farm_total_staked {}\n\
         marinade_lp_farm_total_rewards_per_week {}\n\
         marinade_lp_farm_rewards_per_token {}\n",
        msol_price,
        msol_apy_7d,
        msol_apy_30d,
        msol_apy_365d,
        lp_price,
        lp_apy_1d,
        lp_apy_7d,
        lp_apy_30d,
        lp_apy_365d,
        msol_farm_total_staked,
        msol_farm_total_rewards_per_week,
        msol_farm_rewards_per_token,
        lp_farm_total_staked,
        lp_farm_total_rewards_per_week,
        lp_farm_rewards_per_token,
    ))
}
