use std::convert::{TryFrom, TryInto};

use anyhow::anyhow;
use chrono::{DateTime, TimeZone, Utc};
#[cfg(feature = "solana")]
use cli_common::anchor_lang::AccountDeserialize;
use cli_common::marinade_finance::{liq_pool::LiqPoolHelpers, State};
#[cfg(feature = "solana")]
use cli_common::solana_client::{rpc_client::RpcClient, rpc_response::Response};
#[cfg(feature = "solana")]
use cli_common::solana_sdk::{
    account::ReadableAccount, commitment_config::CommitmentConfig, program_pack::Pack,
};
use cli_common::WithKey;

#[cfg(feature = "postgres")]
use postgres::types::ToSql;

#[derive(Debug, Clone)]
pub struct BalancesSample {
    pub slot: u64,
    pub slot_time: DateTime<Utc>,
    pub reserve_balance: Option<u64>,
    pub active_stakes_balance: Option<u64>,
    pub cooling_down_stakes_balance: Option<u64>,
    pub emergency_unstaking: Option<u64>,
    pub claim_tickets_balance: Option<u64>,
    pub total_virtual_staked_lamports: u64,
    pub msol_supply: u64,
    pub liq_pool_sol_leg_balance: u64,
    pub liq_pool_msol_leg_balance: u64,
    pub lp_supply: u64,
}

impl BalancesSample {
    #[cfg(feature = "solana")]
    pub fn from_blockchain(
        client: &RpcClient,
        state: &mut WithKey<State>,
        commitment: CommitmentConfig,
    ) -> anyhow::Result<Self> {
        use cli_common::spl_token;

        let Response {
            context,
            value: accounts,
        } = client.get_multiple_accounts_with_commitment(
            &[
                state.key,
                state.liq_pool_sol_leg_address(),
                state.liq_pool.msol_leg,
            ],
            commitment,
        )?;

        let state: State = AccountDeserialize::try_deserialize(
            &mut accounts[0]
                .as_ref()
                .ok_or_else(|| anyhow!("Can not find state account"))?
                .data
                .as_slice(),
        )?;
        let liq_pool_sol_leg_balance = accounts[1]
            .as_ref()
            .ok_or_else(|| anyhow!("Can not find sol leg account"))?
            .lamports()
            - state.rent_exempt_for_token_acc;
        let liq_pool_msol_leg = spl_token::state::Account::unpack(
            &accounts[2]
                .as_ref()
                .ok_or_else(|| anyhow!("Can not find msol leg account"))?
                .data,
        )?;
        let slot_time = Utc.timestamp(client.get_block_time(context.slot)?, 0);
        Ok(BalancesSample {
            slot: context.slot,
            slot_time,
            reserve_balance: Some(state.available_reserve_balance),
            active_stakes_balance: Some(state.validator_system.total_active_balance),
            cooling_down_stakes_balance: Some(state.stake_system.delayed_unstake_cooling_down),
            emergency_unstaking: Some(state.emergency_cooling_down),
            claim_tickets_balance: Some(state.circulating_ticket_balance),
            total_virtual_staked_lamports: state.total_virtual_staked_lamports(),
            msol_supply: state.msol_supply,
            liq_pool_sol_leg_balance,
            liq_pool_msol_leg_balance: liq_pool_msol_leg.amount,
            lp_supply: state.liq_pool.lp_supply,
        })
    }

    pub fn msol_price(&self) -> f64 {
        self.total_virtual_staked_lamports as f64 / self.msol_supply as f64
    }

    pub fn lp_value(&self) -> f64 {
        self.liq_pool_sol_leg_balance as f64
            + self.liq_pool_msol_leg_balance as f64 * self.msol_price()
    }

    pub fn lp_price(&self) -> f64 {
        self.lp_value() / self.lp_supply as f64
    }

    #[cfg(feature = "postgres")]
    pub fn db_read_one(
        connection: &mut postgres::Client,
        condition: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> anyhow::Result<Option<Self>> {
        Ok(
            match connection.query_opt(
                format!(
                    "SELECT 
                        slot,
                        slot_time,
                        reserve_balance,
                        active_stakes_balance,
                        cooling_down_stakes_balance,
                        emergency_unstaking,
                        claim_tickets_balance,
                        total_virtual_staked_lamports,
                        msol_supply,
                        liq_pool_sol_leg_balance,
                        liq_pool_msol_leg_balance,
                        lp_supply
                    FROM balances {} LIMIT 1",
                    condition
                )
                .as_str(),
                params,
            )? {
                Some(row) => {
                    let slot: i64 = row.get(0);
                    let slot_time: DateTime<Utc> = row.get(1);
                    let reserve_balance: Option<i64> = row.get(2);
                    let active_stakes_balance: Option<i64> = row.get(3);
                    let cooling_down_stakes_balance: Option<i64> = row.get(4);
                    let emergency_unstaking: Option<i64> = row.get(5);
                    let claim_tickets_balance: Option<i64> = row.get(6);
                    let total_virtual_staked_lamports: i64 = row.get(7);
                    let msol_supply: i64 = row.get(8);
                    let liq_pool_sol_leg_balance: i64 = row.get(9);
                    let liq_pool_msol_leg_balance: i64 = row.get(10);
                    let lp_supply: i64 = row.get(11);
                    Some(Self {
                        slot: slot.try_into()?,
                        slot_time,
                        reserve_balance: if let Some(reserve_balance) = reserve_balance {
                            Some(reserve_balance.try_into()?)
                        } else {
                            None
                        },
                        active_stakes_balance: if let Some(active_stakes_balance) =
                            active_stakes_balance
                        {
                            Some(active_stakes_balance.try_into()?)
                        } else {
                            None
                        },
                        cooling_down_stakes_balance: if let Some(cooling_down_stakes_balance) =
                            cooling_down_stakes_balance
                        {
                            Some(cooling_down_stakes_balance.try_into()?)
                        } else {
                            None
                        },
                        emergency_unstaking: if let Some(emergency_unstaking) = emergency_unstaking
                        {
                            Some(emergency_unstaking.try_into()?)
                        } else {
                            None
                        },
                        claim_tickets_balance: if let Some(claim_tickets_balance) =
                            claim_tickets_balance
                        {
                            Some(claim_tickets_balance.try_into()?)
                        } else {
                            None
                        },
                        total_virtual_staked_lamports: total_virtual_staked_lamports.try_into()?,
                        msol_supply: msol_supply.try_into()?,
                        liq_pool_sol_leg_balance: liq_pool_sol_leg_balance.try_into()?,
                        liq_pool_msol_leg_balance: liq_pool_msol_leg_balance.try_into()?,
                        lp_supply: lp_supply.try_into()?,
                    })
                }
                None => None,
            },
        )
    }

    #[cfg(feature = "postgres")]
    pub fn db_read_last(connection: &mut postgres::Client) -> anyhow::Result<Self> {
        Self::db_read_one(connection, "ORDER BY slot DESC", &[])?.ok_or_else(|| anyhow!("No data"))
    }

    #[cfg(feature = "postgres")]
    pub fn db_read_last_before(
        connection: &mut postgres::Client,
        requested_time: DateTime<Utc>,
    ) -> anyhow::Result<Option<Self>> {
        Self::db_read_one(
            connection,
            "WHERE slot_time <= $1 ORDER BY slot DESC",
            &[&requested_time],
        )
    }

    #[cfg(feature = "postgres")]
    pub fn db_read_first(connection: &mut postgres::Client) -> anyhow::Result<Option<Self>> {
        Self::db_read_one(connection, "ORDER BY slot", &[])
    }

    #[cfg(feature = "postgres")]
    pub fn db_read_last_before_or_first(
        connection: &mut postgres::Client,
        requested_time: DateTime<Utc>,
    ) -> anyhow::Result<Self> {
        Ok(
            if let Some(sample) = Self::db_read_last_before(connection, requested_time)? {
                sample
            } else {
                Self::db_read_first(connection)?.ok_or_else(|| anyhow!("No data"))?
            },
        )
    }

    #[cfg(feature = "postgres")]
    pub fn db_save(&self, connection: &mut postgres::Client) -> anyhow::Result<()> {
        const VERSION_ERROR_MESSAGE: &str =
            "Saving of not recent record version from db is not supported";
        let slot = i64::try_from(self.slot).unwrap();
        let reserve_balance = i64::try_from(self.reserve_balance.expect(VERSION_ERROR_MESSAGE))
            .map_err(|_| anyhow!("reserve balance does not fit into i64"))?;
        let active_stakes_balance =
            i64::try_from(self.active_stakes_balance.expect(VERSION_ERROR_MESSAGE))
                .map_err(|_| anyhow!("active stakes balance does not fit into i64"))?;
        let cooling_down_stakes_balance = i64::try_from(
            self.cooling_down_stakes_balance
                .expect(VERSION_ERROR_MESSAGE),
        )
        .map_err(|_| anyhow!("cooling down stakes balance does not fit into i64"))?;
        let emergency_unstaking =
            i64::try_from(self.emergency_unstaking.expect(VERSION_ERROR_MESSAGE))
                .map_err(|_| anyhow!("emergency unstaking does not fit into i64"))?;
        let claim_tickets_balance =
            i64::try_from(self.claim_tickets_balance.expect(VERSION_ERROR_MESSAGE))
                .map_err(|_| anyhow!("claim tickets balance does not fit into i64"))?;
        let virtual_staking_lamports = i64::try_from(self.total_virtual_staked_lamports)
            .map_err(|_| anyhow!("total virtual staked balance does not fit into i64"))?;
        let msol_supply = i64::try_from(self.msol_supply)
            .map_err(|_| anyhow!("msol supply does not fit into i64"))?;
        let liq_pool_sol_leg_balance = i64::try_from(self.liq_pool_sol_leg_balance)
            .map_err(|_| anyhow!("liq pool SOL leg balance does not fit into i64"))?;
        let liq_pool_msol_leg_balance = i64::try_from(self.liq_pool_msol_leg_balance)
            .map_err(|_| anyhow!("liq pool mSOL leg balance does not fit into i64"))?;
        let lp_supply = i64::try_from(self.lp_supply)
            .map_err(|_| anyhow!("lp supply does not fit into i64"))?;
        connection.execute(
            "INSERT INTO balances(
            slot,
            slot_time,
            reserve_balance,
            active_stakes_balance,
            cooling_down_stakes_balance,
            emergency_unstaking,
            claim_tickets_balance,
            total_virtual_staked_lamports,
            msol_supply,
            liq_pool_sol_leg_balance,
            liq_pool_msol_leg_balance,
            lp_supply)
        VALUES($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
            &[
                &slot,
                &self.slot_time,
                &reserve_balance,
                &active_stakes_balance,
                &cooling_down_stakes_balance,
                &emergency_unstaking,
                &claim_tickets_balance,
                &virtual_staking_lamports,
                &msol_supply,
                &liq_pool_sol_leg_balance,
                &liq_pool_msol_leg_balance,
                &lp_supply,
            ],
        )?;
        Ok(())
    }
}
