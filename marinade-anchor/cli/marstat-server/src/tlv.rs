use crate::error::Result;
use crate::{get_sol_price, MarstatConnection};
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use marstat::BalancesSample;
use rocket::serde::{json::Json, Serialize};
use solana_sdk::native_token::lamports_to_sol;

#[derive(Clone, Debug, Serialize)]
pub struct Tlv {
    staked_sol: f64,
    staked_usd: f64,
    liquidity_sol: f64,
    liquidity_usd: f64,
    total_sol: f64,
    total_usd: f64,
}

fn get_tlv(
    connection: &mut postgres::Client,
    sol_price: f64,
    time: Option<DateTime<Utc>>,
) -> Result<Tlv> {
    let sample = if let Some(time) = time {
        BalancesSample::db_read_last_before(connection, time)?
            .ok_or_else(|| anyhow!("No data for {}", time))?
    } else {
        BalancesSample::db_read_last(connection)?
    };
    let staked = sample
        .active_stakes_balance
        .ok_or_else(|| anyhow!("No active_stakes_balance data"))?
        + sample
            .cooling_down_stakes_balance
            .ok_or_else(|| anyhow!("No cooling_down_stakes_balance data"))?
        + sample
            .reserve_balance
            .ok_or_else(|| anyhow!("No reserve_balance data"))?;
    Ok(Tlv {
        staked_sol: lamports_to_sol(staked),
        staked_usd: lamports_to_sol(staked) * sol_price,
        liquidity_sol: lamports_to_sol(sample.liq_pool_sol_leg_balance),
        liquidity_usd: lamports_to_sol(sample.liq_pool_sol_leg_balance) * sol_price,
        total_sol: lamports_to_sol(staked + sample.liq_pool_sol_leg_balance),
        total_usd: lamports_to_sol(staked + sample.liq_pool_sol_leg_balance) * sol_price,
    })
}

#[get("/tlv?<time>")]
pub async fn tlv(connection: MarstatConnection, time: Option<String>) -> Result<Json<Tlv>> {
    let sol_price = get_sol_price().await?;
    let time = time.map_or(Ok(None), |time| {
        DateTime::parse_from_rfc3339(&time)
            .map(|dt| dt.with_timezone(&Utc))
            .map(Some)
    })?;
    Ok(Json(
        connection.run(move |c| get_tlv(c, sol_price, time)).await?,
    ))
}
