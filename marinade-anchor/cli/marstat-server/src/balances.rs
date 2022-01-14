use anyhow::anyhow;
use chrono::{DateTime, Utc};
use marstat::BalancesSample;

use crate::{error::Result, get_sol_price, MarstatConnection};

fn get_lp_price(
    connection: &mut postgres::Client,
    time: Option<DateTime<Utc>>,
) -> anyhow::Result<f64> {
    Ok(if let Some(time) = time {
        BalancesSample::db_read_last_before(connection, time)?
            .ok_or_else(|| anyhow!("No data for {}", time))?
    } else {
        BalancesSample::db_read_last(connection)?
    }
    .lp_price())
}

#[get("/lp/price?<time>")]
pub async fn lp_price(connection: MarstatConnection, time: Option<String>) -> Result<String> {
    let time = time.map_or(Ok(None), |time| {
        DateTime::parse_from_rfc3339(&time)
            .map(|dt| dt.with_timezone(&Utc))
            .map(Some)
    })?;
    Ok(connection
        .run(move |c| get_lp_price(c, time))
        .await?
        .to_string())
}

fn get_msol_supply(connection: &mut postgres::Client, time: Option<DateTime<Utc>>) -> Result<u64> {
    Ok(if let Some(time) = time {
        BalancesSample::db_read_last_before(connection, time)?
            .ok_or_else(|| anyhow!("No data for {}", time))?
    } else {
        BalancesSample::db_read_last(connection)?
    }
    .msol_supply)
}

#[get("/msol/supply?<time>")]
pub async fn msol_supply(connection: MarstatConnection, time: Option<String>) -> Result<String> {
    let time = time.map_or(Ok(None), |time| {
        DateTime::parse_from_rfc3339(&time)
            .map(|dt| dt.with_timezone(&Utc))
            .map(Some)
    })?;
    connection
        .run(move |c| get_msol_supply(c, time))
        .await
        .map(|v| v.to_string())
}

fn get_msol_price_sol(
    connection: &mut postgres::Client,
    time: Option<DateTime<Utc>>,
) -> Result<f64> {
    let sample = if let Some(time) = time {
        BalancesSample::db_read_last_before(connection, time)?
            .ok_or_else(|| anyhow!("No data for {}", time))?
    } else {
        BalancesSample::db_read_last(connection)?
    };
    Ok(sample.msol_price())
}

#[get("/msol/price_sol?<time>")]
pub async fn msol_price_sol(connection: MarstatConnection, time: Option<String>) -> Result<String> {
    let time = time.map_or(Ok(None), |time| {
        DateTime::parse_from_rfc3339(&time)
            .map(|dt| dt.with_timezone(&Utc))
            .map(Some)
    })?;
    connection
        .run(move |c| get_msol_price_sol(c, time))
        .await
        .map(|v| v.to_string())
}

#[get("/msol/price_usd")]
pub async fn msol_price_usd(connection: MarstatConnection) -> Result<String> {
    let sol_price = get_sol_price().await?;
    connection
        .run(move |c| get_msol_price_sol(c, None))
        .await
        .map(|v| (v * sol_price).to_string())
}
