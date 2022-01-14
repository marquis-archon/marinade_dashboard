use std::convert::TryFrom;
use std::str::FromStr;

use anyhow::anyhow;
use chrono::{DateTime, TimeZone, Utc};
#[cfg(feature = "solana")]
use cli_common::solana_client::{rpc_client::RpcClient, rpc_response::Response};
use cli_common::solana_sdk::commitment_config::CommitmentConfig;
use cli_common::solana_sdk::pubkey::Pubkey;
use lazy_static::lazy_static;
use postgres::types::ToSql;
#[cfg(feature = "postgres")]
use quarry_mine::{Quarry, Rewarder};

#[derive(Debug, Clone)]
pub struct Farms {
    pub slot: u64,
    pub slot_time: DateTime<Utc>,
    pub staked_token: Pubkey,
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

lazy_static! {
    pub static ref REWARDER_PUBKEY: Pubkey =
        Pubkey::from_str("J829VB5Fi7DMoMLK7bsVGFM82cRU61BKtiPz9PNFdL7b").unwrap();
    pub static ref STAKED_TOKENS: Vec<Pubkey> = vec![
        Pubkey::from_str("mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So").unwrap(),
        Pubkey::from_str("LPmSozJJ8Jh69ut2WP3XmVohTjL4ipR18yiCzxrUmVj").unwrap()
    ];
}

impl Farms {
    #[cfg(feature = "solana")]
    pub fn from_blockchain(
        client: &RpcClient,
        commitment: CommitmentConfig,
    ) -> anyhow::Result<Vec<Self>> {
        use quarry_anchor_lang::AccountDeserialize;
        use quarry_mine::payroll::PRECISION_MULTIPLIER;

        let mut account_keys: Vec<Pubkey> = vec![*REWARDER_PUBKEY];
        // Add quaries
        account_keys.extend(STAKED_TOKENS.iter().map(|staked_token| {
            Pubkey::find_program_address(
                &[
                    b"Quarry".as_ref(),
                    REWARDER_PUBKEY.to_bytes().as_ref(),
                    staked_token.to_bytes().as_ref(),
                ],
                &quarry_mine::ID,
            )
            .0
        }));

        let Response {
            context,
            value: accounts,
        } = client.get_multiple_accounts_with_commitment(&account_keys, commitment)?;

        let slot_time = Utc.timestamp(client.get_block_time(context.slot)?, 0);

        let _rewarder = Rewarder::try_deserialize(
            &mut accounts[0]
                .as_ref()
                .ok_or_else(|| anyhow!("Can not read rewarder {}", *REWARDER_PUBKEY))?
                .data
                .as_slice(),
        )?;

        let mut result = vec![];

        for i in 0..STAKED_TOKENS.len() {
            let quarry = Quarry::try_deserialize(
                &mut accounts[i + 1]
                    .as_ref()
                    .ok_or_else(|| anyhow!("Can not read quarry for {}", STAKED_TOKENS[i]))?
                    .data
                    .as_slice(),
            )?;

            let last_update = Utc.timestamp(quarry.last_update_ts, 0);

            let rewards_per_token_stored =
                rug::Rational::from((quarry.rewards_per_token_stored, PRECISION_MULTIPLIER))
                    .to_f64();

            result.push(Self {
                slot: context.slot,
                slot_time,
                staked_token: STAKED_TOKENS[i],
                last_update,
                annual_rewards_rate: quarry.annual_rewards_rate,
                total_tokens_deposited: quarry.total_tokens_deposited,
                num_miners: quarry.num_miners,
                rewards_per_token_stored,
            })
        }

        Ok(result)
    }

    #[cfg(feature = "postgres")]
    pub fn db_read_one(
        connection: &mut postgres::Client,
        condition: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> anyhow::Result<Option<Self>> {
        use std::convert::TryInto;

        Ok(
            match connection.query_opt(
                format!(
                    "SELECT 
                    slot,
                    slot_time,
                    staked_token,
                    last_update,
                    annual_rewards_rate,
                    total_tokens_deposited,
                    num_miners,
                    rewards_per_token_stored
                    FROM farms {} LIMIT 1",
                    condition
                )
                .as_str(),
                params,
            )? {
                Some(row) => {
                    let slot: i64 = row.get(0);
                    let slot_time: DateTime<Utc> = row.get(1);
                    let staked_token: String = row.get(2);
                    let last_update: DateTime<Utc> = row.get(3);
                    let annual_rewards_rate: i64 = row.get(4);
                    let total_tokens_deposited: i64 = row.get(5);
                    let num_miners: i64 = row.get(6);
                    let rewards_per_token_stored: f64 = row.get(7);

                    Some(Self {
                        slot: slot.try_into()?,
                        slot_time,
                        staked_token: Pubkey::from_str(&staked_token)?,
                        last_update,
                        annual_rewards_rate: annual_rewards_rate.try_into()?,
                        total_tokens_deposited: total_tokens_deposited.try_into()?,
                        num_miners: num_miners.try_into()?,
                        rewards_per_token_stored,
                    })
                }
                None => None,
            },
        )
    }

    #[cfg(feature = "postgres")]
    pub fn db_read_last(
        connection: &mut postgres::Client,
        staked_token: &Pubkey,
    ) -> anyhow::Result<Self> {
        Self::db_read_one(
            connection,
            "WHERE staked_token = $1 ORDER BY slot DESC",
            &[&staked_token.to_string()],
        )?
        .ok_or_else(|| anyhow!("No data"))
    }

    #[cfg(feature = "postgres")]
    pub fn db_read_last_before(
        connection: &mut postgres::Client,
        staked_token: &Pubkey,
        requested_time: DateTime<Utc>,
    ) -> anyhow::Result<Option<Self>> {
        Self::db_read_one(
            connection,
            "WHERE staked_token = $1 AND slot_time <= $2 ORDER BY slot DESC",
            &[&staked_token.to_string(), &requested_time],
        )
    }

    #[cfg(feature = "postgres")]
    pub fn db_read_first(
        connection: &mut postgres::Client,
        staked_token: &Pubkey,
    ) -> anyhow::Result<Option<Self>> {
        Self::db_read_one(
            connection,
            "WHERE staked_token = $1 ORDER BY slot",
            &[&staked_token.to_string()],
        )
    }

    #[cfg(feature = "postgres")]
    pub fn db_read_last_before_or_first(
        connection: &mut postgres::Client,
        staked_token: &Pubkey,
        requested_time: DateTime<Utc>,
    ) -> anyhow::Result<Self> {
        Ok(
            if let Some(sample) =
                Self::db_read_last_before(connection, staked_token, requested_time)?
            {
                sample
            } else {
                Self::db_read_first(connection, staked_token)?.ok_or_else(|| anyhow!("No data"))?
            },
        )
    }

    #[cfg(feature = "postgres")]
    pub fn db_save(&self, connection: &mut postgres::Client) -> anyhow::Result<()> {
        let slot = i64::try_from(self.slot).unwrap();
        let staked_token = self.staked_token.to_string();
        let annual_rewards_rate = i64::try_from(self.annual_rewards_rate).unwrap();
        let total_tokens_deposited = i64::try_from(self.total_tokens_deposited).unwrap();
        let num_miners = i64::try_from(self.num_miners).unwrap();
        connection.execute(
            "INSERT INTO farms(
            slot,
            slot_time,
            staked_token,
            last_update,
            annual_rewards_rate,
            total_tokens_deposited,
            num_miners,
            rewards_per_token_stored)
        VALUES($1, $2, $3, $4, $5, $6, $7, $8)",
            &[
                &slot,
                &self.slot_time,
                &staked_token,
                &self.last_update,
                &annual_rewards_rate,
                &total_tokens_deposited,
                &num_miners,
                &self.rewards_per_token_stored,
            ],
        )?;
        Ok(())
    }
}
