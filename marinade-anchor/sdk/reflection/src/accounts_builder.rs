use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
};

use anyhow::{anyhow, bail};
use marinade_finance_offchain_sdk::solana_sdk::{
    account::Account,
    clock::Epoch,
    native_token::LAMPORTS_PER_SOL,
    program_option::COption,
    program_pack::Pack,
    stake::{
        self,
        state::{Authorized, Delegation, Lockup, Meta, Stake, StakeState},
    },
    system_program,
};
use marinade_finance_offchain_sdk::WithKey;
use marinade_finance_offchain_sdk::{anchor_lang::prelude::*, marinade_finance};
use marinade_finance_offchain_sdk::{
    marinade_finance::{
        liq_pool::LiqPoolHelpers,
        located::Located,
        stake_system::{StakeRecord, StakeSystem, StakeSystemHelpers},
        state::StateHelpers,
        ticket_account::TicketAccountData,
        validator_system::{ValidatorRecord, ValidatorSystem},
    },
    spl_token,
};
use more_asserts::assert_ge;
use rand::{
    distributions::Uniform,
    prelude::{Distribution, SliceRandom},
    RngCore,
};
use solana_vote_program::vote_state::{VoteInit, VoteState, VoteStateVersions};

use crate::{marinade::Marinade, random_pubkey};

#[derive(Debug, Clone)]
pub struct StakeBuilder {
    pub address: Pubkey,
    pub voter_pubkey: Pubkey,
    pub stake: u64,
    pub is_active: bool,
    pub last_update_delegated_lamports: u64,
    pub last_update_epoch: u64,
    pub extra_balance: u64,
}

#[derive(Debug, Clone)]
pub struct ValidatorBuilder {
    pub vote_address: Pubkey,
    pub vote_state: VoteState,
    pub current_stakes: u32,
    pub current_active_balance: u64,
    pub current_delegated_delta: u64,
    pub current_extra_balance: u64,
}
#[derive(Debug, Clone)]
pub struct AccountsBuilder<'a> {
    pub marinade: &'a Marinade,
    pub instance: Pubkey,
    pub stake_list_account: Pubkey,
    pub additional_stake_record_space: u32,
    pub stakes: Vec<StakeBuilder>,
    pub current_total_cooling_down: u64,
    pub current_cooling_down_stakes: u32,
    pub validator_list_account: Pubkey,
    pub additional_validator_record_space: u32,
    pub validators: Vec<ValidatorBuilder>,
    pub treasury_msol_authority: Pubkey,
    pub liq_pool_msol_leg: Pubkey,
    pub target_epoch: u64,
}

#[derive(Debug, Clone)]
pub struct MarinadeAccounts {
    pub state: Pubkey,
    pub stake_list: Pubkey,
    pub validator_list: Pubkey,
    pub validators: HashSet<Pubkey>,
    pub active_stakes: HashSet<Pubkey>,
    pub cooling_down_stakes: HashSet<Pubkey>,
    pub claim_tickets: HashSet<Pubkey>,
    pub target_epoch: u64,

    pub storage: HashMap<Pubkey, Account>,
}

impl<'a> AccountsBuilder<'a> {
    pub const DEFAULT_TARGET_EPOCH: u64 = 256;

    pub fn new_random(
        marinade: &'a Marinade,
        rng: &mut impl RngCore,
        additional_stake_record_space: u32,
        additional_validator_record_space: u32,
    ) -> Self {
        Self {
            marinade,
            instance: random_pubkey(rng),
            stake_list_account: random_pubkey(rng),
            additional_stake_record_space,
            stakes: Vec::new(),
            current_total_cooling_down: 0,
            current_cooling_down_stakes: 0,
            validator_list_account: random_pubkey(rng),
            additional_validator_record_space,
            validators: Vec::new(),
            treasury_msol_authority: random_pubkey(rng),
            liq_pool_msol_leg: random_pubkey(rng),
            target_epoch: Self::DEFAULT_TARGET_EPOCH,
        }
    }

    pub fn add_validator(
        &mut self,
        vote_address: Pubkey,
        vote_state: VoteState,
    ) -> anyhow::Result<()> {
        if !self.marinade.validators.contains_key(&vote_address) {
            bail!("Unknown validator {}", vote_address);
        }
        if self
            .validators
            .iter()
            .any(|validator| validator.vote_address == vote_address)
        {
            bail!("Validator {} duplication", vote_address)
        }
        self.validators.push(ValidatorBuilder {
            vote_address,
            vote_state,
            current_stakes: 0,
            current_active_balance: 0,
            current_delegated_delta: 0,
            current_extra_balance: 0,
        });
        Ok(())
    }

    pub fn shuffle_validators(&mut self, rng: &mut impl RngCore) {
        self.validators.shuffle(rng);
    }

    pub fn add_stake(&mut self, stake: StakeBuilder) -> anyhow::Result<()> {
        if stake.stake < self.marinade.min_stake {
            bail!(
                "Too low stake {}. Need to be at least {}",
                stake.stake,
                self.marinade.min_stake
            );
        }
        if self
            .stakes
            .iter()
            .any(|old_stake| old_stake.address == stake.address)
        {
            bail!("Stake {} is duplicated", stake.address);
        }
        if stake.is_active {
            if let Some(validator_reflection) = self.marinade.validators.get(&stake.voter_pubkey) {
                if let Some(validator_builder) = self
                    .validators
                    .iter_mut()
                    .find(|validator| validator.vote_address == stake.voter_pubkey)
                {
                    if validator_builder.current_stakes >= validator_reflection.stake_count {
                        bail!("Too many stakes for validator {}", stake.voter_pubkey);
                    }
                    if validator_builder.current_active_balance
                        + stake.last_update_delegated_lamports
                        > validator_reflection.active_balance
                    {
                        bail!("Too high validator {} total balance", stake.voter_pubkey);
                    }

                    // Delegate delta is not checked for overflow because delta can be negative

                    if validator_builder.current_extra_balance + stake.extra_balance
                        > validator_reflection.total_extra_balance
                    {
                        bail!("Too high validator {} extra balance", stake.voter_pubkey);
                    }

                    validator_builder.current_stakes += 1;
                    validator_builder.current_active_balance +=
                        stake.last_update_delegated_lamports;
                    validator_builder.current_delegated_delta = validator_builder
                        .current_delegated_delta
                        .checked_add(stake.stake)
                        .expect("delegate_delta overflow")
                        .checked_sub(stake.last_update_delegated_lamports)
                        .expect("negative delegate_delta is not supported");
                    validator_builder.current_extra_balance += stake.extra_balance;

                    self.stakes.push(stake);
                } else {
                    bail!("Add validator {} to builder first", stake.voter_pubkey);
                }
            } else {
                bail!("Unknown stake validator {}", stake.voter_pubkey);
            }
        } else {
            // Stake is cooling down
            if self.current_cooling_down_stakes >= self.marinade.cooling_down_stakes {
                bail!("Too many cooling down stakes");
            }
            if self.current_total_cooling_down + stake.last_update_delegated_lamports
                > self.marinade.total_cooling_down
            {
                bail!("Too high total cooling down");
            }
            self.current_cooling_down_stakes += 1;
            self.current_total_cooling_down += stake.last_update_delegated_lamports;
            // TODO: cooling down stats
            self.stakes.push(stake);
        }

        Ok(())
    }

    pub fn shuffle_stakes(&mut self, rng: &mut impl RngCore) {
        self.stakes.shuffle(rng);
    }

    pub fn random_fill(&mut self, rng: &mut impl RngCore) {
        let clock = Clock::default(); // TODO?

        // Build cooling down stakes
        for i in self.current_cooling_down_stakes..self.marinade.cooling_down_stakes {
            let delegated = if i + 1 < self.marinade.cooling_down_stakes {
                let max_delegated = (self.marinade.total_cooling_down
                    - self.current_total_cooling_down)
                    .checked_sub(
                        (self.marinade.cooling_down_stakes - i - 1) as u64
                            * self.marinade.min_stake,
                    )
                    .expect("Cooling down total must be enough for stake count");
                assert_ge!(max_delegated, self.marinade.min_stake);
                Uniform::from(self.marinade.min_stake..=max_delegated).sample(rng)
            } else {
                let left = self.marinade.total_cooling_down - self.current_total_cooling_down;
                assert_ge!(left, self.marinade.min_stake);
                left
            };
            self.stakes.push(StakeBuilder {
                address: random_pubkey(rng),
                voter_pubkey: random_pubkey(rng),
                stake: delegated, // TODO: extra stake
                is_active: false,
                last_update_delegated_lamports: delegated,
                last_update_epoch: 0, // TODO
                extra_balance: Uniform::from(LAMPORTS_PER_SOL..5 * LAMPORTS_PER_SOL).sample(rng),
            });
            self.current_total_cooling_down += delegated;
        }
        self.current_cooling_down_stakes = self.marinade.cooling_down_stakes;

        // Build validators
        for (validator_key, validator_reflection) in &self.marinade.validators {
            let validator_builder = if let Some(validator_builder) = self
                .validators
                .iter_mut()
                .find(|validator| &validator.vote_address == validator_key)
            {
                validator_builder
            } else {
                let validator_identity = random_pubkey(rng);
                self.validators.push(ValidatorBuilder {
                    vote_address: *validator_key,
                    vote_state: VoteState::new(
                        &VoteInit {
                            node_pubkey: validator_identity,
                            authorized_voter: validator_identity,
                            ..VoteInit::default()
                        },
                        &clock,
                    ),
                    current_stakes: 0,
                    current_active_balance: 0,
                    current_delegated_delta: 0,
                    current_extra_balance: 0,
                });
                self.validators.last_mut().unwrap()
            };

            // Build validator stakes
            assert_ge!(
                validator_reflection.active_balance,
                validator_reflection.stake_count as u64 * self.marinade.min_stake
            );
            for i in validator_builder.current_stakes..validator_reflection.stake_count {
                let max_delegated = validator_reflection.active_balance
                    - validator_builder.current_active_balance
                    - self.marinade.min_stake * (validator_reflection.stake_count - i - 1) as u64;
                assert_ge!(max_delegated, self.marinade.min_stake);
                let delegated = if i + 1 < validator_reflection.stake_count {
                    Uniform::from(self.marinade.min_stake..=max_delegated).sample(rng)
                } else {
                    max_delegated
                };

                let delegated_delta_left = validator_reflection.total_delegated_delta
                    - validator_builder.current_delegated_delta;
                let delegated_delta = if i + 1 < validator_reflection.stake_count {
                    Uniform::from(0..=delegated_delta_left).sample(rng)
                } else {
                    delegated_delta_left
                };

                let extra_balance_left = validator_reflection.total_extra_balance
                    - validator_builder.current_extra_balance;
                let extra_balance = if i + 1 < validator_reflection.stake_count {
                    Uniform::from(0..=extra_balance_left).sample(rng)
                } else {
                    extra_balance_left
                };

                validator_builder.current_stakes += 1;
                validator_builder.current_active_balance += delegated;
                validator_builder.current_delegated_delta += delegated_delta;
                validator_builder.current_extra_balance += extra_balance;

                self.stakes.push(StakeBuilder {
                    address: random_pubkey(rng),
                    voter_pubkey: *validator_key,
                    stake: delegated + delegated_delta,
                    is_active: true,
                    last_update_delegated_lamports: delegated,
                    last_update_epoch: 0, // TODO
                    extra_balance,
                });
            }
        }

        self.shuffle_validators(rng);
        self.shuffle_stakes(rng);
    }

    pub fn build(&self, rent: &Rent) -> anyhow::Result<MarinadeAccounts> {
        if self.validators.len() != self.marinade.validators.len() {
            bail!(
                "Wrong validator count {} expected {}",
                self.validators.len(),
                self.marinade.validators.len()
            );
        }
        if self.current_cooling_down_stakes != self.marinade.cooling_down_stakes {
            bail!(
                "Wrong cooling down stakes {} expected {}",
                self.current_cooling_down_stakes,
                self.marinade.cooling_down_stakes
            );
        }
        if self.current_total_cooling_down != self.marinade.total_cooling_down {
            bail!(
                "Wrong total cooling down {} expected {}",
                self.current_total_cooling_down,
                self.marinade.total_cooling_down
            );
        }
        assert_eq!(self.stakes.len() as u32, self.marinade.stake_count());

        let state = WithKey::new(
            self.marinade.state(
                self.instance,
                self.stake_list_account,
                self.additional_stake_record_space,
                self.validator_list_account,
                self.additional_validator_record_space,
                self.liq_pool_msol_leg,
                rent,
            ),
            self.instance,
        );
        let mut storage = HashMap::new();
        // Marinade state account
        let mut state_data = vec![];
        state
            .as_ref()
            .try_serialize(&mut Cursor::new(&mut state_data))?;
        let mut state_account = Account::new(
            rent.minimum_balance(state_data.len()),
            state_data.len(),
            &marinade_finance_offchain_sdk::marinade_finance::ID,
        );
        state_account.data.copy_from_slice(&state_data);

        storage.insert(self.instance, state_account);

        // Reserve account
        storage.insert(
            state.reserve_address(),
            Account::new(self.marinade.actual_reserve_balance, 0, &system_program::ID),
        );

        // mSOL mint
        if storage
            .insert(
                self.marinade.msol_mint,
                create_mint_account(
                    state.msol_mint_authority(),
                    self.marinade.actual_msol_supply,
                    rent,
                ),
            )
            .is_some()
        {
            bail!(
                "mSOL mint account pubkey duplication {}",
                self.marinade.msol_mint
            );
        }

        // treasury mSOL account
        if storage
            .insert(
                self.marinade.treasury_msol_account,
                create_token_account(
                    self.marinade.msol_mint,
                    self.treasury_msol_authority,
                    0,
                    rent,
                ),
            )
            .is_some()
        {
            bail!(
                "Treasury mSOL account pubkey duplication {}",
                self.marinade.treasury_msol_account
            );
        }

        // lp mint
        if storage
            .insert(
                self.marinade.liq_pool.lp_mint,
                create_mint_account(
                    state.lp_mint_authority(),
                    self.marinade.liq_pool.actual_lp_supply,
                    rent,
                ),
            )
            .is_some()
        {
            bail!(
                "lp mint account pubkey duplication {}",
                self.marinade.liq_pool.lp_mint
            )
        }

        // liq pool sol leg
        if storage
            .insert(
                state.liq_pool_sol_leg_address(),
                Account::new(
                    self.marinade.liq_pool.actual_sol_amount,
                    0,
                    &system_program::ID,
                ),
            )
            .is_some()
        {
            bail!(
                "liq pool SOL leg account pubkey duplication {}",
                state.liq_pool_sol_leg_address()
            );
        }

        // liq pool mSOL leg
        if storage
            .insert(
                self.liq_pool_msol_leg,
                create_token_account(
                    self.marinade.msol_mint,
                    state.liq_pool_msol_leg_authority(),
                    self.marinade.liq_pool.actual_msol_amount,
                    rent,
                ),
            )
            .is_some()
        {
            bail!(
                "liq pool mSOL leg account pubkey duplication {}",
                self.liq_pool_msol_leg
            )
        }

        let stake_list_length = StakeSystem::bytes_for_list(
            self.marinade.max_stakes,
            self.additional_stake_record_space,
        ) as usize;
        let mut stake_list_account = Account::new(
            rent.minimum_balance(stake_list_length),
            stake_list_length,
            &marinade_finance::ID,
        );
        // Account magic number
        stake_list_account.data[0..8].copy_from_slice(StakeRecord::DISCRIMINATOR);

        let mut active_stakes = HashSet::new();
        let mut cooling_down_stakes = HashSet::new();

        let stake_meta = Meta {
            rent_exempt_reserve: StakeState::get_rent_exempt_reserve(rent),
            authorized: Authorized {
                staker: state.stake_deposit_authority(),
                withdrawer: state.stake_withdraw_authority(),
            },
            lockup: Lockup::default(),
        };
        for (index, stake) in self.stakes.iter().enumerate() {
            if stake.is_active {
                active_stakes.insert(stake.address);
            } else {
                cooling_down_stakes.insert(stake.address);
            }
            let stake_record = StakeRecord {
                stake_account: stake.address,
                last_update_delegated_lamports: stake.last_update_delegated_lamports,
                last_update_epoch: stake.last_update_epoch,
                is_emergency_unstaking: 0,
            };
            state
                .stake_system
                .set(&mut stake_list_account.data, index as u32, stake_record)
                .map_err(|err| anyhow!("stake list set failure {}", err))?;
            let stake_state = StakeState::Stake(
                stake_meta,
                Stake {
                    delegation: Delegation {
                        voter_pubkey: stake.voter_pubkey,
                        stake: stake.stake,
                        activation_epoch: 0, //
                        deactivation_epoch: if stake.is_active {
                            Epoch::MAX
                        } else {
                            self.target_epoch
                        },
                        warmup_cooldown_rate: 0.25, // TODO
                    },
                    credits_observed: 0, // TODO
                },
            );
            let mut stake_account = Account::new(
                stake.stake + stake_meta.rent_exempt_reserve + stake.extra_balance,
                std::mem::size_of::<StakeState>(),
                &stake::program::ID,
            );
            let stake_data = bincode::serialize(&stake_state)
                .map_err(|err| anyhow!("stake state serialization fail {}", err))?;
            stake_account.data[0..stake_data.len()].copy_from_slice(&stake_data);
            if storage.insert(stake.address, stake_account).is_some() {
                bail!("Stake account pubkey duplication {}", stake.address);
            }
        }
        // Add stake list account to blockhain
        if storage
            .insert(self.stake_list_account, stake_list_account)
            .is_some()
        {
            bail!(
                "Stake list account pubkey duplication {}",
                self.stake_list_account
            );
        }

        let validator_list_length = ValidatorSystem::bytes_for_list(
            self.marinade.max_validators,
            self.additional_validator_record_space,
        ) as usize;
        let mut validator_list_account = Account::new(
            rent.minimum_balance(validator_list_length),
            validator_list_length,
            &marinade_finance::ID,
        );
        // Account magic number
        validator_list_account.data[0..8].copy_from_slice(ValidatorRecord::DISCRIMINATOR);

        for (index, validator) in self.validators.iter().enumerate() {
            let validator_reflection = self
                .marinade
                .validators
                .get(&validator.vote_address)
                .ok_or_else(|| anyhow!("Unknown validator {}", validator.vote_address))?;
            if validator.current_active_balance != validator_reflection.active_balance {
                bail!("Wrong validator {} active balance", validator.vote_address);
            }
            if validator.current_stakes != validator_reflection.stake_count {
                bail!("Wrong validator {} stake number", validator.vote_address);
            }
            if validator.current_delegated_delta != validator_reflection.total_delegated_delta {
                bail!("Wrong validator {} delegated delta", validator.vote_address)
            }
            if validator.current_extra_balance != validator_reflection.total_extra_balance {
                bail!("Wrong validator {} extra balance", validator.vote_address)
            }
            let (duplication_flag_address, duplication_flag_bump_seed) =
                ValidatorRecord::find_duplication_flag(&self.instance, &validator.vote_address);
            let validator_record = ValidatorRecord {
                validator_account: validator.vote_address,
                active_balance: validator_reflection.active_balance,
                score: validator_reflection.score,
                last_stake_delta_epoch: validator_reflection.last_stake_delta_epoch,
                duplication_flag_bump_seed,
            };
            // Save validator to list
            state.validator_system.set(
                &mut validator_list_account.data,
                index as u32,
                validator_record,
            )?;

            // Add validator identity account to blockchain
            if storage
                .insert(
                    validator.vote_state.node_pubkey,
                    Account::new(rent.minimum_balance(0), 0, &system_program::ID),
                )
                .is_some()
            {
                bail!(
                    "Validator identity {} duplication",
                    validator.vote_state.node_pubkey
                );
            }

            // Add validator vote account to blockchain
            let mut vote_account = Account::new(
                rent.minimum_balance(VoteState::size_of()),
                VoteState::size_of(),
                &solana_vote_program::id(),
            );
            VoteState::to(
                &VoteStateVersions::Current(Box::new(validator.vote_state.clone())),
                &mut vote_account,
            )
            .ok_or_else(|| anyhow!("Vote serialize fail"))?;
            if storage
                .insert(validator.vote_address, vote_account)
                .is_some()
            {
                bail!("Validator vote {} duplication", validator.vote_address);
            }

            // Mark validator as added by initializing duplication flag account
            if storage
                .insert(
                    duplication_flag_address,
                    Account::new(rent.minimum_balance(0), 0, &marinade_finance::ID),
                )
                .is_some()
            {
                bail!(
                    "Duplication flag duplicated, lol {}",
                    duplication_flag_address
                );
            }
        }
        // Add validator list account to blockchain
        if storage
            .insert(self.validator_list_account, validator_list_account)
            .is_some()
        {
            bail!(
                "Validator list account pubkey duplication {}",
                self.validator_list_account
            )
        }

        for (key, claim_ticket) in &self.marinade.claim_tickets {
            let ticket = TicketAccountData {
                state_address: self.instance,
                beneficiary: claim_ticket.beneficiary,
                lamports_amount: claim_ticket.lamports_amount,
                created_epoch: claim_ticket.created_epoch,
            };
            let mut ticket_data = vec![];
            ticket.try_serialize(&mut Cursor::new(&mut ticket_data))?;
            let mut ticket_account = Account::new(
                rent.minimum_balance(ticket_data.len()),
                ticket_data.len(),
                &marinade_finance::ID,
            );
            ticket_account.data.copy_from_slice(&ticket_data);

            if storage.insert(*key, ticket_account).is_some() {
                bail!("Ticket account {} duplication", key);
            }
        }

        Ok(MarinadeAccounts {
            state: self.instance,
            stake_list: self.stake_list_account,
            validator_list: self.validator_list_account,
            validators: self.marinade.validators.keys().cloned().collect(),
            active_stakes,
            cooling_down_stakes,
            claim_tickets: self.marinade.claim_tickets.keys().cloned().collect(),
            target_epoch: self.target_epoch,
            storage,
        })
    }
}

pub fn create_mint_account(authority: Pubkey, supply: u64, rent: &Rent) -> Account {
    let mut mint_account = Account::new(
        rent.minimum_balance(spl_token::state::Mint::LEN),
        spl_token::state::Mint::LEN,
        &spl_token::ID,
    );
    let mint_state = spl_token::state::Mint {
        mint_authority: COption::Some(authority),
        supply,
        decimals: 9,
        is_initialized: true,
        freeze_authority: COption::None,
    };
    mint_state.pack_into_slice(&mut mint_account.data);
    mint_account
}

pub fn create_token_account(mint: Pubkey, owner: Pubkey, amount: u64, rent: &Rent) -> Account {
    let mut token_account = Account::new(
        rent.minimum_balance(spl_token::state::Account::LEN),
        spl_token::state::Account::LEN,
        &spl_token::ID,
    );
    let state = spl_token::state::Account {
        mint,
        owner,
        amount,
        delegate: COption::None,
        state: spl_token::state::AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };
    state.pack_into_slice(&mut token_account.data);
    token_account
}
