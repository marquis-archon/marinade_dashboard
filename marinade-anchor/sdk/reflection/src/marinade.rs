use std::collections::BTreeMap;
use std::iter::FromIterator;

use crate::liq_pool::LiqPool;
use crate::pubkey_map;
use crate::FeeDef;
use anyhow::{anyhow, bail};
use marinade_finance_offchain_sdk::anchor_lang::prelude::*;
use marinade_finance_offchain_sdk::marinade_finance;
use marinade_finance_offchain_sdk::marinade_finance::{
    liq_pool::LiqPoolHelpers,
    list::List,
    stake_system::{StakeRecord, StakeSystem},
    state::StateHelpers,
    ticket_account::TicketAccountData,
    validator_system::{ValidatorRecord, ValidatorSystem},
    Fee, State,
};
use marinade_finance_offchain_sdk::solana_sdk::{
    clock::Epoch, program_pack::Pack, stake::state::StakeState,
};
use marinade_finance_offchain_sdk::spl_token;
use marinade_finance_offchain_sdk::WithKey;
use serde::Serialize;
use solana_program_test::BanksClient;

#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct Validator {
    pub active_balance: u64,
    pub stake_count: u32,
    pub score: u32,
    pub last_stake_delta_epoch: u64,
    // difference between actual total delegated and active_balance recorded field
    pub total_delegated_delta: u64,
    // total not delegates lamports on stakes
    pub total_extra_balance: u64,
}

#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct ClaimTicket {
    pub beneficiary: Pubkey,  // main account where to send SOL when claimed
    pub lamports_amount: u64, // amount this ticked is worth
    pub created_epoch: u64, // epoch when this acc was created (epoch when delayed-unstake was requested)
}

#[derive(Debug, PartialEq, Clone, Serialize)]
pub struct Marinade {
    pub msol_mint: Pubkey,

    pub admin_authority: Pubkey,

    // Target for withdrawing rent reserve SOLs. Save bot wallet account here
    pub operational_sol_account: Pubkey,
    // treasury - external accounts managed by marinade DAO
    pub treasury_msol_account: Pubkey,

    pub min_stake: u64, // Minimal stake amount

    // fee applied on rewards
    #[serde(with = "FeeDef")]
    pub reward_fee: Fee,

    pub validator_manager_authority: Pubkey,
    #[serde(with = "pubkey_map")]
    pub validators: BTreeMap<Pubkey, Validator>,
    pub max_validators: u32,
    pub total_cooling_down: u64,
    pub cooling_down_stakes: u32,
    /* TODO:
    // difference between actual total delegated and active_balance recorded field (normally positive)
    pub cooling_down_delegated_delta: i128,
    // total not delegates lamports on stakes
    pub cooling_down_extra_balance: u64,
    */
    pub max_stakes: u32,

    pub liq_pool: LiqPool,
    pub available_reserve_balance: u64, // reserve_pda.lamports() - self.rent_exempt_for_token_acc. Virtual value (real may be > because of transfers into reserve). Use Update* to align
    pub actual_reserve_balance: u64,
    pub msol_supply: u64, // Virtual value (may be < because of token burn). Use Update* to align
    pub actual_msol_supply: u64,

    #[serde(with = "pubkey_map")]
    pub claim_tickets: BTreeMap<Pubkey, ClaimTicket>,
    pub slots_for_stake_delta: u64,
    pub last_stake_delta_epoch: u64,
    pub lent_from_reserve: u64,
    pub min_deposit: u64,
    pub min_withdraw: u64,
    pub staking_sol_cap: u64,
    pub liquidity_sol_cap: u64,
}

impl Marinade {
    pub fn validator_count(&self) -> u32 {
        self.validators.len() as u32
    }

    pub fn stake_count(&self) -> u32 {
        self.validators
            .values()
            .map(|validator| validator.stake_count)
            .sum::<u32>()
            + self.cooling_down_stakes
    }

    pub fn total_validator_score(&self) -> u32 {
        self.validators
            .values()
            .map(|validator| validator.score)
            .sum()
    }

    pub fn total_active_balance(&self) -> u64 {
        self.validators
            .values()
            .map(|validator| validator.active_balance)
            .sum()
    }

    pub fn circulating_ticket_balance(&self) -> u64 {
        self.claim_tickets
            .iter()
            .map(|(_, ticket)| ticket.lamports_amount)
            .sum()
    }

    pub async fn read_from_test(
        banks_client: &mut BanksClient,
        instance: &Pubkey,
        claim_tickets_iter: impl IntoIterator<Item = Pubkey>,
    ) -> anyhow::Result<Self> {
        let state_account = banks_client
            .get_account(*instance)
            .await?
            .ok_or_else(|| anyhow!("Marinade {} not found", instance))?;
        let state: WithKey<State> = WithKey::new(
            AccountDeserialize::try_deserialize(&mut state_account.data.as_slice())?,
            *instance,
        );
        let validator_list = banks_client
            .get_account(*state.validator_system.validator_list_address())
            .await?
            .ok_or_else(|| {
                anyhow!(
                    "Marinade {} validator list {} not found",
                    instance,
                    state.validator_system.validator_list_address()
                )
            })?
            .data;

        let mut validators = BTreeMap::new();
        for i in 0..state.validator_system.validator_count() {
            let validator = state.validator_system.get(&validator_list, i)?;
            if validators
                .insert(
                    validator.validator_account,
                    Validator {
                        active_balance: validator.active_balance,
                        stake_count: 0, // Will be counted later
                        score: validator.score,
                        last_stake_delta_epoch: validator.last_stake_delta_epoch,
                        total_delegated_delta: 0,
                        total_extra_balance: 0,
                    },
                )
                .is_some()
            {
                bail!("Validator {} duplication", validator.validator_account)
            }
        }

        let stake_list = banks_client
            .get_account(*state.stake_system.stake_list_address())
            .await?
            .ok_or_else(|| {
                anyhow!(
                    "Marinade {} validator list {} not found",
                    instance,
                    state.stake_system.stake_list_address()
                )
            })?
            .data;

        let mut total_cooling_down = 0;
        let mut cooling_down_stakes = 0;

        for i in 0..state.stake_system.stake_count() {
            let stake_record = state.stake_system.get(&stake_list, i)?;

            let stake_account = banks_client
                .get_account(stake_record.stake_account)
                .await?
                .ok_or_else(|| {
                    anyhow!(
                        "Marinade {} stake {} not found",
                        instance,
                        stake_record.stake_account
                    )
                })?;

            let stake_state =
                bincode::deserialize::<StakeState>(&stake_account.data).map_err(|err| {
                    anyhow!(
                        "Error reading stake {}: {}",
                        stake_record.stake_account,
                        err
                    )
                })?;

            let stake_delegation = stake_state.delegation().ok_or_else(|| {
                anyhow!(
                    "Undelegated stake {} under control",
                    stake_record.stake_account
                )
            })?;
            if stake_delegation.deactivation_epoch == Epoch::MAX {
                let stake_validator = validators
                    .get_mut(&stake_delegation.voter_pubkey)
                    .ok_or_else(|| {
                        anyhow!(
                            "Validator {} is not registered in marinade {}",
                            stake_delegation.voter_pubkey,
                            instance
                        )
                    })?;

                stake_validator.stake_count += 1;
                stake_validator.total_delegated_delta += stake_delegation
                    .stake
                    .checked_sub(stake_record.last_update_delegated_lamports)
                    .expect("Slashing is not supported");
                stake_validator.total_extra_balance += stake_account.lamports
                    - stake_delegation.stake
                    - stake_state.meta().unwrap().rent_exempt_reserve;
            } else {
                total_cooling_down += stake_record.last_update_delegated_lamports;
                cooling_down_stakes += 1;
                // TODO: cooling down stats
            }
        }

        let actual_reserve_balance = banks_client
            .get_account(state.reserve_address())
            .await?
            .map_or(0, |account| account.lamports);

        let actual_msol_supply =
            if let Some(msol_mint) = banks_client.get_account(state.msol_mint).await? {
                spl_token::state::Mint::unpack(&msol_mint.data)?.supply
            } else {
                0
            };

        let actual_lp_supply =
            if let Some(lp_mint) = banks_client.get_account(state.liq_pool.lp_mint).await? {
                spl_token::state::Mint::unpack(&lp_mint.data)?.supply
            } else {
                0
            };
        let actual_liq_pool_sol_amount = banks_client
            .get_account(state.liq_pool_sol_leg_address())
            .await?
            .map_or(0, |account| account.lamports);
        let actual_liq_pool_msol_amount = if let Some(liq_pool_msol_leg) =
            banks_client.get_account(state.liq_pool.msol_leg).await?
        {
            spl_token::state::Account::unpack(&liq_pool_msol_leg.data)?.amount
        } else {
            0
        };

        let mut claim_tickets = BTreeMap::new();
        for key in claim_tickets_iter {
            let ticket_data: TicketAccountData = AccountDeserialize::try_deserialize(
                &mut banks_client
                    .get_account(key)
                    .await?
                    .ok_or_else(|| anyhow!("Can not find ticket account {}", key))?
                    .data
                    .as_slice(),
            )
            .map_err(|err| anyhow!("Error parsing ticket {}: {}", key, err))?;
            if &ticket_data.state_address != instance {
                bail!("Wrong ticket owner");
            }
            claim_tickets.insert(
                key,
                ClaimTicket {
                    beneficiary: ticket_data.beneficiary,
                    lamports_amount: ticket_data.lamports_amount,
                    created_epoch: ticket_data.created_epoch,
                },
            );
        }
        Ok(Self {
            msol_mint: state.msol_mint,
            admin_authority: state.admin_authority,
            operational_sol_account: state.operational_sol_account,
            treasury_msol_account: state.treasury_msol_account,
            min_stake: state.stake_system.min_stake,
            reward_fee: state.reward_fee,
            validator_manager_authority: state.validator_system.manager_authority,
            validators,
            max_validators: state
                .validator_system
                .validator_list_capacity(validator_list.len())?,
            total_cooling_down,
            cooling_down_stakes,
            max_stakes: state.stake_system.stake_list_capacity(stake_list.len())?,
            liq_pool: LiqPool {
                lp_mint: state.liq_pool.lp_mint,
                actual_sol_amount: actual_liq_pool_sol_amount,
                actual_msol_amount: actual_liq_pool_msol_amount,
                lp_liquidity_target: state.liq_pool.lp_liquidity_target,
                lp_max_fee: state.liq_pool.lp_max_fee,
                lp_min_fee: state.liq_pool.lp_min_fee,
                treasury_cut: state.liq_pool.treasury_cut,
                lp_supply: state.liq_pool.lp_supply,
                actual_lp_supply,
                lent_from_sol_leg: state.liq_pool.lent_from_sol_leg,
            },
            available_reserve_balance: state.available_reserve_balance,
            actual_reserve_balance,
            msol_supply: state.msol_supply,
            actual_msol_supply,
            claim_tickets,
            slots_for_stake_delta: state.stake_system.slots_for_stake_delta,
            last_stake_delta_epoch: state.stake_system.last_stake_delta_epoch,
            lent_from_reserve: state.lent_from_reserve,
            min_deposit: state.min_deposit,
            min_withdraw: state.min_withdraw,
            staking_sol_cap: state.staking_sol_cap,
            liquidity_sol_cap: state.liq_pool.liquidity_sol_cap,
        })
    }

    pub fn state(
        &self,
        instance: Pubkey,
        stake_list_account: Pubkey,
        additional_stake_record_space: u32,
        validator_list_account: Pubkey,
        additional_validator_record_space: u32,
        liq_pool_msol_leg: Pubkey,
        rent: &Rent,
    ) -> State {
        let stake_system = StakeSystem {
            stake_list: List {
                account: stake_list_account,
                item_size: StakeRecord::default().try_to_vec().unwrap().len() as u32
                    + additional_stake_record_space,
                count: self.stake_count(),
                new_account: Pubkey::default(),
                copied_count: 0,
            },
            delayed_unstake_cooling_down: self.total_cooling_down,
            stake_deposit_bump_seed: StakeSystem::find_stake_deposit_authority(&instance).1,
            stake_withdraw_bump_seed: StakeSystem::find_stake_withdraw_authority(&instance).1,

            slots_for_stake_delta: self.slots_for_stake_delta,
            min_stake: self.min_stake,
            extra_stake_delta_runs: 0,
            last_stake_delta_epoch: self.last_stake_delta_epoch,
        };

        let validator_system = ValidatorSystem {
            validator_list: List {
                account: validator_list_account,
                item_size: ValidatorRecord::default().try_to_vec().unwrap().len() as u32
                    + additional_validator_record_space,
                count: self.validator_count(),
                new_account: Pubkey::default(),
                copied_count: 0,
            },
            manager_authority: self.validator_manager_authority,
            total_validator_score: self.total_validator_score(),
            total_active_balance: self.total_active_balance(),
            auto_add_validator_enabled: 0,
        };
        State {
            msol_mint: self.msol_mint,
            admin_authority: self.admin_authority,
            operational_sol_account: self.operational_sol_account,
            treasury_msol_account: self.treasury_msol_account,
            reserve_bump_seed: State::find_reserve_address(&instance).1,
            msol_mint_authority_bump_seed: State::find_msol_mint_authority(&instance).1,
            rent_exempt_for_token_acc: rent.minimum_balance(spl_token::state::Account::LEN),
            reward_fee: self.reward_fee,
            stake_system,
            validator_system,
            liq_pool: marinade_finance::liq_pool::LiqPool {
                lp_mint: self.liq_pool.lp_mint,
                lp_mint_authority_bump_seed:
                    marinade_finance::liq_pool::LiqPool::find_lp_mint_authority(&instance).1,
                sol_leg_bump_seed: marinade_finance::liq_pool::LiqPool::find_sol_leg_address(
                    &instance,
                )
                .1,
                msol_leg_authority_bump_seed:
                    marinade_finance::liq_pool::LiqPool::find_msol_leg_authority(&instance).1,
                msol_leg: liq_pool_msol_leg,
                lp_liquidity_target: self.liq_pool.lp_liquidity_target,
                lp_max_fee: self.liq_pool.lp_max_fee,
                lp_min_fee: self.liq_pool.lp_min_fee,
                treasury_cut: self.liq_pool.treasury_cut,
                lp_supply: self.liq_pool.lp_supply,
                lent_from_sol_leg: self.liq_pool.lent_from_sol_leg,
                liquidity_sol_cap: self.liquidity_sol_cap,
            },
            available_reserve_balance: self.available_reserve_balance,
            msol_supply: self.msol_supply,
            msol_price: 1, // TODO
            circulating_ticket_count: self.claim_tickets.len() as u64,
            circulating_ticket_balance: self.circulating_ticket_balance(),
            lent_from_reserve: self.lent_from_reserve,
            min_deposit: self.min_deposit,
            min_withdraw: self.min_withdraw,
            staking_sol_cap: self.staking_sol_cap,
            emergency_cooling_down: 0,
        }
    }

    pub fn validator_keys<C: FromIterator<Pubkey>>(&self) -> C {
        self.validators.keys().cloned().collect()
    }

    pub fn claim_ticket_keys<C: FromIterator<Pubkey>>(&self) -> C {
        self.claim_tickets.keys().cloned().collect()
    }
}
