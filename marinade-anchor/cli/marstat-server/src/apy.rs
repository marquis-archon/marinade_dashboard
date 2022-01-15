use crate::error::Result;
use anyhow::{anyhow, bail};
use chrono::{DateTime, Duration, Utc};
use duration_string::DurationString;
use float_duration::FloatDuration;
use marstat::BalancesSample;
use rocket::serde::json::Json;
use serde::Serialize;

use crate::MarstatConnection;

pub fn calc_apy(
    end_time: DateTime<Utc>,
    end_price: f64,
    start_time: DateTime<Utc>,
    start_price: f64,
) -> f64 {
    assert!(end_time > start_time);
    let count = FloatDuration::years(1.0) / FloatDuration::from_chrono(end_time - start_time);
    (end_price / start_price).powf(count) - 1.0
}

#[derive(Clone, Debug, Serialize)]
pub struct Apy {
    value: f64,
    end_time: DateTime<Utc>,
    end_price: f64,
    start_time: DateTime<Utc>,
    start_price: f64,
}

fn get_msol_apy(
    connection: &mut postgres::Client,
    requested_period: Duration,
    time: Option<DateTime<Utc>>,
) -> anyhow::Result<Apy> {
    let current_sample = if let Some(time) = time {
        BalancesSample::db_read_last_before(connection, time)?
            .ok_or_else(|| anyhow!("No data for {}", time))?
    } else {
        BalancesSample::db_read_last(connection)?
    };

    let requested_prev_time = current_sample.slot_time - requested_period;
    let prev_sample =
        BalancesSample::db_read_last_before_or_first(connection, requested_prev_time)?;
    if prev_sample.slot >= current_sample.slot {
        bail!("Not enough data");
    }
    let current_msol_pirce = current_sample.msol_price();
    let prev_msol_price = prev_sample.msol_price();
    Ok(Apy {
        value: calc_apy(
            current_sample.slot_time,
            current_msol_pirce,
            prev_sample.slot_time,
            prev_msol_price,
        ),
        end_time: current_sample.slot_time,
        end_price: current_msol_pirce,
        start_time: prev_sample.slot_time,
        start_price: prev_msol_price,
    })
}

#[get("/msol/apy/<period>?<time>")]
pub async fn msol_apy(
    connection: MarstatConnection,
    period: String,
    time: Option<String>,
) -> Result<Json<Apy>> {
    let period = Duration::from_std(
        DurationString::from_string(period)
            .map_err(|e| anyhow!("duration parsing error: {}", e))?
            .into(),
    )?;
    let time = time.map_or(Ok(None), |time| {
        DateTime::parse_from_rfc3339(&time)
            .map(|dt| dt.with_timezone(&Utc))
            .map(Some)
    })?;
    Ok(Json(
        connection
            .run(move |c| get_msol_apy(c, period, time))
            .await?,
    ))
}

fn get_lp_apy(
    connection: &mut postgres::Client,
    requested_period: Duration,
    time: Option<DateTime<Utc>>,
) -> anyhow::Result<Apy> {
    let current_sample = if let Some(time) = time {
        BalancesSample::db_read_last_before(connection, time)?
            .ok_or_else(|| anyhow!("No data for {}", time))?
    } else {
        BalancesSample::db_read_last(connection)?
    };

    let requested_prev_time = current_sample.slot_time - requested_period;
    let prev_sample =
        BalancesSample::db_read_last_before_or_first(connection, requested_prev_time)?;
    if prev_sample.slot >= current_sample.slot {
        bail!("Not enough data");
    }
    let current_lp_pirce = current_sample.lp_price();
    let prev_lp_price = prev_sample.lp_price();
    Ok(Apy {
        value: calc_apy(
            current_sample.slot_time,
            current_lp_pirce,
            prev_sample.slot_time,
            prev_lp_price,
        ),
        end_time: current_sample.slot_time,
        end_price: current_lp_pirce,
        start_time: prev_sample.slot_time,
        start_price: prev_lp_price,
    })
}

#[get("/lp/apy/<period>?<time>")]
pub async fn lp_apy(
    connection: MarstatConnection,
    period: String,
    time: Option<String>,
) -> Result<Json<Apy>> {
    let period = Duration::from_std(
        DurationString::from_string(period)
            .map_err(|e| anyhow!("duration parsing error: {}", e))?
            .into(),
    )?;
    let time = time.map_or(Ok(None), |time| {
        DateTime::parse_from_rfc3339(&time)
            .map(|dt| dt.with_timezone(&Utc))
            .map(Some)
    })?;
    Ok(Json(
        connection.run(move |c| get_lp_apy(c, period, time)).await?,
    ))
}
