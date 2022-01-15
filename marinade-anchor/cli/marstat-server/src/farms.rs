use crate::{error::Result, MarstatConnection, ParsedPubkey};
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use marstat::Farms;
use rocket::serde::json::Json;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Farm {
    pub last_update: DateTime<Utc>,
    /// Amount of rewards distributed to the quarry per year.
    pub annual_rewards_rate: u64,

    /// Total number of tokens deposited into the quarry.
    pub total_tokens_deposited: u64,
    /// Number of [Miner]s.
    pub num_miners: u64,
    /// Rewards per token stored in the quarry
    pub rewards_per_token_stored: f64,
}

#[get("/farm/<token>?<time>")]
pub async fn get_farm(
    connection: MarstatConnection,
    token: ParsedPubkey,
    time: Option<String>,
) -> Result<Json<Farm>> {
    let time = time.map_or(Ok(None), |time| {
        DateTime::parse_from_rfc3339(&time)
            .map(|dt| dt.with_timezone(&Utc))
            .map(Some)
    })?;
    connection
        .run(move |connection| {
            let farm = if let Some(time) = time {
                Farms::db_read_last_before(connection, &token, time)?
                    .ok_or_else(|| anyhow!("No data for {}", time))?
            } else {
                Farms::db_read_last(connection, &token)?
            };
            Ok(Json(Farm {
                last_update: farm.last_update,
                annual_rewards_rate: farm.annual_rewards_rate,
                total_tokens_deposited: farm.total_tokens_deposited,
                num_miners: farm.num_miners,
                rewards_per_token_stored: farm.rewards_per_token_stored,
            }))
        })
        .await
}
