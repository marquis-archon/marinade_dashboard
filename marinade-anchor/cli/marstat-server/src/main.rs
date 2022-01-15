#![cfg_attr(not(debug_assertions), deny(warnings))]

use std::ops::Deref;
use std::str::FromStr;

use rocket::{http::Method, request::FromParam};

use rocket::serde::Deserialize;
use rocket_cors::{AllowedHeaders, AllowedOrigins};
use rocket_sync_db_pools::{database, postgres};

mod apy;
mod balances;
mod error;
mod farms;
mod metrics;
mod tlv;

use error::{Error, Result};
use solana_sdk::pubkey::{ParsePubkeyError, Pubkey};

#[macro_use]
extern crate rocket;

#[database("marstat")]
pub struct MarstatConnection(postgres::Client);

#[rocket::main]
async fn main() -> anyhow::Result<()> {
    let allowed_origins = AllowedOrigins::all(); // AllowedOrigins::some_exact(&["https://marinade.finance"]);

    // You can also deserialize this
    let cors = rocket_cors::CorsOptions {
        allowed_origins,
        allowed_methods: vec![Method::Get].into_iter().map(From::from).collect(),
        allowed_headers: AllowedHeaders::some(&["Accept"]),
        allow_credentials: true,
        ..Default::default()
    }
    .to_cors()?;

    rocket::build()
        .attach(MarstatConnection::fairing())
        .attach(cors)
        .mount(
            "/",
            routes![
                balances::lp_price,
                apy::msol_apy,
                apy::lp_apy,
                metrics::metrics,
                balances::msol_supply,
                tlv::tlv,
                balances::msol_price_sol,
                balances::msol_price_usd,
                farms::get_farm,
            ],
        )
        .launch()
        .await?;
    Ok(())
}

pub async fn get_sol_price() -> Result<f64> {
    #[derive(Deserialize)]
    struct BinanceResponse {
        #[allow(dead_code)]
        mins: f64,
        price: String,
    }
    let binance_response = reqwest::get("https://api.binance.com/api/v3/avgPrice?symbol=SOLBUSD")
        .await?
        .json::<BinanceResponse>()
        .await?;
    Ok(binance_response.price.parse()?)
}

pub struct ParsedPubkey(pub Pubkey);

impl Deref for ParsedPubkey {
    type Target = Pubkey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> FromParam<'a> for ParsedPubkey {
    type Error = ParsePubkeyError;

    fn from_param(param: &'a str) -> std::result::Result<Self, Self::Error> {
        Ok(Self(Pubkey::from_str(param)?))
    }
}
