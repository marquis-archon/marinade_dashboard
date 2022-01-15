use std::{collections::BTreeMap, iter::FromIterator, ops::Range};

use anyhow::{anyhow, bail};
use marinade_finance_offchain_sdk::marinade_finance::{Fee, MAX_REWARD_FEE};
use marinade_finance_offchain_sdk::solana_sdk::program_pack::Pack;
use marinade_finance_offchain_sdk::solana_sdk::{
    clock::Epoch, native_token::LAMPORTS_PER_SOL, pubkey::Pubkey, rent::Rent,
};
use marinade_finance_offchain_sdk::spl_token;
use more_asserts::{assert_ge, assert_le};
use once_cell::unsync::OnceCell;
use rand::{distributions::Uniform, prelude::Distribution, RngCore};

use crate::{
    liq_pool::LiqPool,
    marinade::{ClaimTicket, Marinade, Validator},
    random_pubkey,
};

#[derive(Debug, Default)]
pub struct Builder {
    pub msol_mint: OnceCell<Pubkey>,

    pub admin_authority: OnceCell<Pubkey>,

    // Target for withdrawing rent reserve SOLs. Save bot wallet account here
    pub operational_sol_account: OnceCell<Pubkey>,
    // treasury - external accounts managed by marinade DAO
    pub treasury_msol_account: OnceCell<Pubkey>,

    pub min_stake: OnceCell<u64>, // Minimal stake amount

    // fee applied on rewards
    pub reward_fee: OnceCell<Fee>,

    pub validator_manager_authority: OnceCell<Pubkey>,
    pub validators: BTreeMap<Pubkey, Validator>,
    pub free_validator_slots: OnceCell<u32>,
    pub total_cooling_down: OnceCell<u64>,
    pub cooling_down_stakes: OnceCell<u32>,
    pub free_stake_slots: OnceCell<u32>,

    // Liq pool
    pub lp_mint: OnceCell<Pubkey>,
    pub lp_supply: OnceCell<u64>,
    pub actual_lp_supply: OnceCell<u64>,
    pub actual_liq_pool_sol_amount: OnceCell<u64>,
    pub actual_liq_pool_msol_amount: OnceCell<u64>,
    pub lp_liquidity_target: OnceCell<u64>,
    pub lp_max_fee: OnceCell<Fee>,
    pub lp_min_fee: OnceCell<Fee>,
    pub lp_treasury_cut: OnceCell<Fee>,
    pub lent_from_liq_pool: OnceCell<u64>,

    pub available_reserve_balance: OnceCell<u64>, // reserve_pda.lamports() - self.rent_exempt_for_token_acc. Virtual value (real may be > because of transfers into reserve). Use Update* to align
    pub actual_reserve_balance: OnceCell<u64>,
    pub msol_supply: OnceCell<u64>, // Virtual value (may be < because of token burn). Use Update* to align
    pub actual_msol_supply: OnceCell<u64>,

    pub claim_tickets: BTreeMap<Pubkey, ClaimTicket>,
    pub slots_for_stake_delta: OnceCell<u64>,
    pub last_stake_delta_epoch: OnceCell<u64>,
    pub lent_from_reserve: OnceCell<u64>,
    pub min_deposit: OnceCell<u64>,
    pub min_withdraw: OnceCell<u64>,
    pub staking_sol_cap: OnceCell<u64>,
    pub liquidity_sol_cap: OnceCell<u64>,
}

pub struct RandomBuildParams {
    pub added_empty_validator_count: Range<u32>,
    pub added_used_validator_count: Range<u32>,
    pub added_validator_stake_count: Range<u32>,
    pub added_total_active_balance: Range<u64>,
    pub added_total_delegate_delta: Range<u64>,
    pub added_total_extra_balance: Range<u64>,
}

impl Default for RandomBuildParams {
    fn default() -> Self {
        Self {
            added_empty_validator_count: 0..1,
            added_used_validator_count: 0..1,
            added_validator_stake_count: 0..1,
            added_total_active_balance: 0..1,
            added_total_delegate_delta: 0..1,
            added_total_extra_balance: 0..1,
        }
    }
}

impl RandomBuildParams {
    pub fn pick(builder: &mut Builder, rng: &mut impl RngCore) -> Self {
        let added_used_validator_count = 5..10;
        let added_validator_stake_count = 2..5;
        let min_total_active_balance = (added_used_validator_count.end - 1) as u64
            * (added_validator_stake_count.end - 1) as u64
            * *builder
                .min_stake
                .get_or_init(|| Uniform::from(LAMPORTS_PER_SOL..10 * LAMPORTS_PER_SOL).sample(rng));
        Self {
            added_empty_validator_count: 2..4,
            added_used_validator_count,
            added_validator_stake_count,
            added_total_active_balance: min_total_active_balance
                ..(min_total_active_balance + 10_000 * LAMPORTS_PER_SOL),
            added_total_delegate_delta: LAMPORTS_PER_SOL..10 * LAMPORTS_PER_SOL,
            added_total_extra_balance: LAMPORTS_PER_SOL..10 * LAMPORTS_PER_SOL,
        }
    }

    fn check(&self, builder: &Builder) {
        assert!(!self.added_empty_validator_count.is_empty());
        assert!(!self.added_used_validator_count.is_empty());
        assert!(!self.added_validator_stake_count.is_empty());
        assert!(!self.added_total_active_balance.is_empty());
        assert!(!self.added_total_delegate_delta.is_empty());
        assert!(!self.added_total_extra_balance.is_empty());

        let max_active_stakes =
            (self.added_used_validator_count.end - 1) * (self.added_validator_stake_count.end - 1);

        assert_ge!(
            self.added_total_active_balance.start,
            max_active_stakes as u64 * *builder.min_stake.get().expect("min_stake must be set")
        );
    }

    pub fn added_empty_validator_count(&self, rng: &mut impl RngCore) -> u32 {
        Uniform::from(self.added_empty_validator_count.clone()).sample(rng)
    }

    pub fn added_used_validator_count(&self, rng: &mut impl RngCore) -> u32 {
        Uniform::from(self.added_used_validator_count.clone()).sample(rng)
    }

    pub fn added_validator_stake_count(&self, rng: &mut impl RngCore) -> u32 {
        Uniform::from(self.added_validator_stake_count.clone()).sample(rng)
    }

    pub fn added_total_active_balance(&self, rng: &mut impl RngCore) -> u64 {
        Uniform::from(self.added_total_active_balance.clone()).sample(rng)
    }

    pub fn added_total_delegate_delta(&self, rng: &mut impl RngCore) -> u64 {
        Uniform::from(self.added_total_delegate_delta.clone()).sample(rng)
    }

    pub fn added_total_extra_balance(&self, rng: &mut impl RngCore) -> u64 {
        Uniform::from(self.added_total_extra_balance.clone()).sample(rng)
    }
}

impl Builder {
    pub fn set_msol_mint(&mut self, mint: Pubkey) {
        self.msol_mint
            .set(mint)
            .expect("double msol_mint set calls");
    }

    pub fn set_admin_authority(&mut self, admin_authority: Pubkey) {
        self.admin_authority
            .set(admin_authority)
            .expect("double admin_authority set calls");
    }

    pub fn set_operational_sol_account(&mut self, operational_sol_account: Pubkey) {
        self.operational_sol_account
            .set(operational_sol_account)
            .expect("double operational_sol_account set calls");
    }

    pub fn set_treasury_msol_account(&mut self, treasury_msol_account: Pubkey) {
        self.treasury_msol_account
            .set(treasury_msol_account)
            .expect("double treasury_msol_account set calls");
    }

    pub fn set_min_stake(&mut self, min_stake: u64) {
        self.min_stake
            .set(min_stake)
            .expect("double min_stake set calls");
    }

    pub fn set_reward_fee(&mut self, reward_fee: Fee) {
        self.reward_fee
            .set(reward_fee)
            .expect("double reward_fee set calls");
    }

    pub fn set_validator_manager_authority(&mut self, validator_manager_authority: Pubkey) {
        self.validator_manager_authority
            .set(validator_manager_authority)
            .expect("double validator_manager_authority set calls");
    }

    pub fn stake_count(&self) -> u32 {
        *self
            .cooling_down_stakes
            .get()
            .expect("cooling down stakes must be set before")
            + self
                .validators
                .values()
                .map(|validator| validator.stake_count)
                .sum::<u32>()
    }

    pub fn add_validator(
        &mut self,
        vote_account: Pubkey,
        validator: Validator,
    ) -> anyhow::Result<()> {
        if self.validators.insert(vote_account, validator).is_none() {
            Ok(())
        } else {
            Err(anyhow!("Validator duplication"))
        }
    }

    pub fn total_active_balance(&self) -> u64 {
        self.validators
            .values()
            .map(|validator| validator.active_balance)
            .sum::<u64>()
    }

    pub fn total_lamports_under_control(&self) -> u64 {
        self.total_active_balance()
            + *self
                .total_cooling_down
                .get()
                .expect("total_cooling_down must be set")
            + *self
                .available_reserve_balance
                .get()
                .expect("available_reserve_balance must be set")
    }

    pub fn total_claim_ordered(&self) -> u64 {
        self.claim_tickets
            .values()
            .map(|claim_ticket| claim_ticket.lamports_amount)
            .sum::<u64>()
    }

    pub fn claim_ticket_keys<C: FromIterator<Pubkey>>(&self) -> C {
        self.claim_tickets.keys().cloned().collect()
    }

    pub fn add_claim_ticket(
        &mut self,
        key: Pubkey,
        claim_ticket: ClaimTicket,
    ) -> anyhow::Result<()> {
        if claim_ticket.lamports_amount + self.total_claim_ordered()
            > self.total_lamports_under_control()
        {
            bail!("Too high claim lamports amount");
        }
        if self.claim_tickets.insert(key, claim_ticket).is_some() {
            bail!("Duplicated claim ticket {}", key);
        }
        Ok(())
    }

    pub fn set_free_validator_slots(&mut self, free_validator_slots: u32) {
        self.free_validator_slots
            .set(free_validator_slots)
            .expect("double free_validator_slots set calls")
    }

    pub fn set_total_cooling_down(&mut self, total_cooling_down: u64) {
        self.total_cooling_down
            .set(total_cooling_down)
            .expect("double total_cooling_down set calls")
    }

    pub fn set_cooling_down_stakes(&mut self, cooling_down_stakes: u32) {
        self.cooling_down_stakes
            .set(cooling_down_stakes)
            .expect("double cooling_down_stakes set calls")
    }

    pub fn set_free_stake_slots(&mut self, free_stake_slots: u32) {
        self.free_stake_slots
            .set(free_stake_slots)
            .expect("double free_stake_slots set calls")
    }

    pub fn set_lp_mint(&mut self, lp_mint: Pubkey) {
        self.lp_mint.set(lp_mint).expect("double lp_mint set calls");
    }

    pub fn set_lp_supply(&mut self, lp_supply: u64) {
        self.lp_supply
            .set(lp_supply)
            .expect("double lp_supply set calls")
    }

    pub fn set_actual_lp_supply(&mut self, actual_lp_supply: u64) {
        self.actual_lp_supply
            .set(actual_lp_supply)
            .expect("double actual_lp_supply set calls")
    }

    pub fn set_actual_liq_pool_sol_amount(&mut self, actual_liq_pool_sol_amount: u64) {
        self.actual_liq_pool_sol_amount
            .set(actual_liq_pool_sol_amount)
            .expect("double actual_liq_pool_sol_amount set calls")
    }

    pub fn set_actual_liq_pool_msol_amount(&mut self, actual_liq_pool_msol_amount: u64) {
        self.actual_liq_pool_msol_amount
            .set(actual_liq_pool_msol_amount)
            .expect("double actual_liq_pool_msol_amount set calls")
    }

    pub fn set_lp_liquidity_target(&mut self, lp_liquidity_target: u64) {
        self.lp_liquidity_target
            .set(lp_liquidity_target)
            .expect("double lp_liquidity_target set calls")
    }

    pub fn set_lp_max_fee(&mut self, lp_max_fee: Fee) {
        self.lp_max_fee
            .set(lp_max_fee)
            .expect("double lp_max_fee set calls")
    }

    pub fn set_lp_min_fee(&mut self, lp_min_fee: Fee) {
        self.lp_min_fee
            .set(lp_min_fee)
            .expect("double lp_min_fee set calls")
    }

    pub fn set_lp_treasury_cut(&mut self, lp_treasury_cut: Fee) {
        self.lp_treasury_cut
            .set(lp_treasury_cut)
            .expect("double lp_treasury_cut set calls")
    }

    pub fn set_lent_from_liq_pool(&mut self, lent_from_liq_pool: u64) {
        self.lent_from_liq_pool
            .set(lent_from_liq_pool)
            .expect("double lent_from_liq_pool set calls")
    }

    pub fn set_available_reserve_balance(&mut self, available_reserve_balance: u64) {
        self.available_reserve_balance
            .set(available_reserve_balance)
            .expect("double available_reserve_balance set calls")
    }

    pub fn set_actual_reserve_balance(&mut self, actual_reserve_balance: u64) {
        self.actual_reserve_balance
            .set(actual_reserve_balance)
            .expect("double actual_reserve_balance set calls")
    }

    pub fn set_msol_supply(&mut self, msol_supply: u64) {
        self.msol_supply
            .set(msol_supply)
            .expect("double msol_supply set calls")
    }

    pub fn set_actual_msol_supply(&mut self, actual_msol_supply: u64) {
        self.actual_msol_supply
            .set(actual_msol_supply)
            .expect("double actual_msol_supply set calls")
    }

    pub fn set_slots_for_stake_delta(&mut self, slots: u64) {
        self.slots_for_stake_delta
            .set(slots)
            .expect("Double slots_for_stake_delta set calls")
    }

    pub fn set_last_stake_delta_epoch(&mut self, last_stake_delta_epoch: u64) {
        self.last_stake_delta_epoch
            .set(last_stake_delta_epoch)
            .expect("double last_stake_delta_epoch set calls");
    }

    pub fn set_min_deposit(&mut self, min_deposit: u64) {
        self.min_deposit
            .set(min_deposit)
            .expect("double min_deposit set calls");
    }

    pub fn set_min_withdraw(&mut self, min_withdraw: u64) {
        self.min_withdraw
            .set(min_withdraw)
            .expect("double min_withdraw set calls")
    }

    pub fn set_staking_sol_cap(&mut self, staking_sol_cap: u64) {
        self.staking_sol_cap
            .set(staking_sol_cap)
            .expect("double staking_sol_cap set calls");
    }

    pub fn set_liquidity_sol_cap(&mut self, liquidity_sol_cap: u64) {
        self.liquidity_sol_cap
            .set(liquidity_sol_cap)
            .expect("double liquidity_sol_cap set calls");
    }

    pub fn add_empty_validator(&mut self, rng: &mut impl RngCore) -> anyhow::Result<Pubkey> {
        let vote = random_pubkey(rng);
        self.add_validator(
            vote,
            Validator {
                active_balance: 0,
                stake_count: 0,
                score: Uniform::from(1..200).sample(rng),
                last_stake_delta_epoch: Epoch::MAX,
                total_delegated_delta: 0,
                total_extra_balance: 0,
            },
        )?;
        Ok(vote)
    }

    pub fn add_random_validator(
        &mut self,
        active_balance: Range<u64>,
        stake_count: Range<u32>,
        total_delegated_delta: Range<u64>,
        total_extra_balance: Range<u64>,
        rng: &mut impl RngCore,
    ) -> anyhow::Result<Pubkey> {
        assert!(!active_balance.is_empty());
        assert!(!stake_count.is_empty());
        assert!(!total_delegated_delta.is_empty());
        assert!(!total_extra_balance.is_empty());

        let vote = random_pubkey(rng);
        self.add_validator(
            vote,
            Validator {
                active_balance: Uniform::from(active_balance).sample(rng),
                stake_count: Uniform::from(stake_count).sample(rng),
                score: Uniform::from(1..200).sample(rng),
                last_stake_delta_epoch: Epoch::MAX,
                total_delegated_delta: Uniform::from(total_delegated_delta).sample(rng),
                total_extra_balance: Uniform::from(total_extra_balance).sample(rng),
            },
        )?;
        Ok(vote)
    }

    pub fn random_fill(&mut self, rng: &mut impl RngCore, params: &RandomBuildParams, rent: &Rent) {
        self.msol_mint.get_or_init(|| random_pubkey(rng));
        self.admin_authority.get_or_init(|| random_pubkey(rng));
        self.operational_sol_account
            .get_or_init(|| random_pubkey(rng));
        self.treasury_msol_account
            .get_or_init(|| random_pubkey(rng));

        self.min_stake
            .get_or_init(|| Uniform::from(LAMPORTS_PER_SOL..10 * LAMPORTS_PER_SOL).sample(rng));

        self.reward_fee
            .get_or_init(|| Fee::from_basis_points(Uniform::from(1..10_00).sample(rng)));

        self.validator_manager_authority
            .get_or_init(|| random_pubkey(rng));
        self.free_validator_slots
            .get_or_init(|| Uniform::from(5..50).sample(rng));
        self.cooling_down_stakes
            .get_or_init(|| Uniform::from(2..5).sample(rng));

        self.total_cooling_down.get_or_init(|| {
            let min_total_cooling_down = *self.min_stake.get().unwrap()
                * (*self.cooling_down_stakes.get().unwrap() as u64 + 1);
            Uniform::from(min_total_cooling_down..(min_total_cooling_down + 100 * LAMPORTS_PER_SOL))
                .sample(rng)
        });
        self.free_stake_slots
            .get_or_init(|| Uniform::from(20..100).sample(rng));
        self.min_deposit
            .get_or_init(|| Uniform::from(LAMPORTS_PER_SOL / 100..LAMPORTS_PER_SOL).sample(rng));
        self.min_withdraw
            .get_or_init(|| Uniform::from(2..LAMPORTS_PER_SOL / 100).sample(rng));

        // Liq pool
        self.lp_mint.get_or_init(|| random_pubkey(rng));
        self.lp_supply.get_or_init(|| {
            let min_lp_supply = *self.min_deposit.get().unwrap() * 100;
            Uniform::from(min_lp_supply..(100_000 * LAMPORTS_PER_SOL + min_lp_supply)).sample(rng)
        });
        self.actual_lp_supply.get_or_init(|| {
            Uniform::from(*self.min_deposit.get().unwrap()..*self.lp_supply.get().unwrap())
                .sample(rng)
        });
        self.actual_liq_pool_sol_amount
            .get_or_init(|| Uniform::from(LAMPORTS_PER_SOL..1_000 * LAMPORTS_PER_SOL).sample(rng));
        self.actual_liq_pool_msol_amount
            .get_or_init(|| Uniform::from(LAMPORTS_PER_SOL..1_000 * LAMPORTS_PER_SOL).sample(rng));
        self.lp_liquidity_target
            .get_or_init(|| Uniform::from((50u64 * LAMPORTS_PER_SOL)..=u64::MAX).sample(rng));
        self.lp_max_fee
            .get_or_init(|| Fee::from_basis_points(Uniform::from(0..MAX_REWARD_FEE).sample(rng)));
        self.lp_min_fee.get_or_init(|| {
            Fee::from_basis_points(
                Uniform::from(self.lp_max_fee.get().unwrap().basis_points..=MAX_REWARD_FEE)
                    .sample(rng),
            )
        });
        self.lp_treasury_cut
            .get_or_init(|| Fee::from_basis_points(Uniform::from(0..MAX_REWARD_FEE).sample(rng)));

        self.available_reserve_balance.get_or_init(|| {
            let min_available_reserve_balance = *self.min_stake.get().unwrap() * 10;
            Uniform::from(
                min_available_reserve_balance
                    ..(100_000 * LAMPORTS_PER_SOL + min_available_reserve_balance),
            )
            .sample(rng)
        });
        self.actual_reserve_balance.get_or_init(|| {
            let min_actual_reserve_balance = *self.available_reserve_balance.get().unwrap()
                + rent.minimum_balance(spl_token::state::Account::LEN);
            Uniform::from(
                min_actual_reserve_balance..min_actual_reserve_balance + 100 * LAMPORTS_PER_SOL,
            )
            .sample(rng)
        });
        self.msol_supply.get_or_init(|| {
            let min_msol_supply = *self.min_deposit.get().unwrap() * 100;
            Uniform::from(min_msol_supply..(100_000 * LAMPORTS_PER_SOL + min_msol_supply))
                .sample(rng)
        });
        self.actual_msol_supply.get_or_init(|| {
            Uniform::from(*self.min_deposit.get().unwrap()..*self.msol_supply.get().unwrap())
                .sample(rng)
        });

        self.slots_for_stake_delta
            .get_or_init(|| Uniform::from(100..1000).sample(rng));
        self.last_stake_delta_epoch.get_or_init(|| Epoch::MAX);

        params.check(self);

        let added_empty_validator_count = params.added_empty_validator_count(rng);
        let added_used_validator_count = params.added_used_validator_count(rng);
        let added_validator_stake_counts: Vec<u32> = (0..added_used_validator_count)
            .map(|_| params.added_validator_stake_count(rng))
            .collect();
        let added_total_active_balance = params.added_total_active_balance(rng);
        let added_total_delegate_delta = params.added_total_delegate_delta(rng);
        let added_total_extra_balance = params.added_total_extra_balance(rng);

        for _ in 0..added_empty_validator_count {
            self.add_empty_validator(rng).unwrap();
        }

        let mut stakes_left_to_add = added_validator_stake_counts.iter().sum::<u32>();
        let mut active_balance_left = added_total_active_balance;
        let mut total_delegated_delta_left = added_total_delegate_delta;
        let mut total_extra_balance_left = added_total_extra_balance;
        for (i, stake_count) in added_validator_stake_counts.iter().enumerate() {
            stakes_left_to_add -= stake_count;
            let min_balance = *stake_count as u64 * *self.min_stake.get().unwrap();
            let max_balance =
                active_balance_left - *self.min_stake.get().unwrap() * stakes_left_to_add as u64;
            assert_ge!(max_balance, min_balance);

            let active_balance = if i + 1 < added_validator_stake_counts.len() {
                Uniform::from(min_balance..=max_balance).sample(rng)
            } else {
                active_balance_left
            };
            active_balance_left -= active_balance;

            let total_delegated_delta = if i + 1 < added_validator_stake_counts.len() {
                Uniform::from(0..=total_delegated_delta_left).sample(rng)
            } else {
                total_delegated_delta_left
            };
            total_delegated_delta_left -= total_delegated_delta;

            let total_extra_balance = if i + 1 < added_validator_stake_counts.len() as usize {
                Uniform::from(0..=total_extra_balance_left).sample(rng)
            } else {
                total_extra_balance_left
            };
            total_extra_balance_left -= total_extra_balance;

            self.add_random_validator(
                active_balance..active_balance + 1,
                *stake_count..*stake_count + 1, // stake count was calculated earlier
                total_delegated_delta..total_delegated_delta + 1,
                total_extra_balance..total_extra_balance + 1,
                rng,
            )
            .unwrap();
        }

        assert_eq!(stakes_left_to_add, 0);
        assert_eq!(active_balance_left, 0);
        assert_eq!(total_delegated_delta_left, 0);

        assert_eq!(total_extra_balance_left, 0);
    }

    pub fn fill_random_claim_tickets(
        &mut self,
        total_lamports: Range<u64>,
        count: Range<usize>,
        min_ticket_amount: u64,
        rng: &mut impl RngCore,
    ) -> anyhow::Result<()> {
        assert!(!total_lamports.is_empty());
        assert!(!count.is_empty());
        if (total_lamports.end - 1) + self.total_claim_ordered()
            > self.total_lamports_under_control()
        {
            bail!("Too high claim lamports amount");
        }

        if total_lamports.start < min_ticket_amount * (count.end - 1) as u64 {
            bail!("Too low total_lamports for min_ticket_amount and count");
        }

        let mut total_lamports = Uniform::from(total_lamports).sample(rng);
        let count = Uniform::from(count).sample(rng);

        for i in 0..count {
            let max_ticket_amount = total_lamports - (count - 1 - i) as u64 * min_ticket_amount;
            assert_ge!(max_ticket_amount, min_ticket_amount);
            let ticket_amount = Uniform::from(min_ticket_amount..=max_ticket_amount).sample(rng);
            total_lamports -= ticket_amount;
            self.add_claim_ticket(
                random_pubkey(rng),
                ClaimTicket {
                    beneficiary: random_pubkey(rng),
                    lamports_amount: ticket_amount,
                    created_epoch: 0, // TODO
                },
            )?;
        }

        Ok(())
    }

    pub fn build(self, rent: &Rent) -> Marinade {
        assert_le!(
            self.total_claim_ordered(),
            self.total_lamports_under_control()
        );
        let stake_count = self.stake_count();
        let validator_count = self.validators.len() as u32;
        let Self {
            msol_mint,
            admin_authority,
            operational_sol_account,
            treasury_msol_account,
            min_stake,
            reward_fee,
            validator_manager_authority,
            validators,
            free_validator_slots,
            total_cooling_down,
            cooling_down_stakes,
            free_stake_slots,
            lp_mint,
            lp_supply,
            actual_lp_supply,
            actual_liq_pool_sol_amount,
            actual_liq_pool_msol_amount,
            lp_liquidity_target,
            lp_max_fee,
            lp_min_fee,
            lp_treasury_cut,
            lent_from_liq_pool,
            available_reserve_balance,
            actual_reserve_balance,
            msol_supply,
            actual_msol_supply,
            claim_tickets,
            slots_for_stake_delta,
            last_stake_delta_epoch,
            lent_from_reserve,
            min_deposit,
            min_withdraw,
            staking_sol_cap,
            liquidity_sol_cap,
        } = self;
        let rent_exempt_for_token_acc = rent.minimum_balance(spl_token::state::Account::LEN);
        let lp_supply = lp_supply.into_inner().expect("lp_supply msut be set");
        let available_reserve_balance = available_reserve_balance
            .into_inner()
            .expect("available_reserve_balance must be set");
        let msol_supply = msol_supply.into_inner().expect("msol_supply must be set");
        Marinade {
            msol_mint: msol_mint.into_inner().expect("msol_mint must be set"),
            admin_authority: admin_authority
                .into_inner()
                .expect("admin_authority must be set"),
            operational_sol_account: operational_sol_account
                .into_inner()
                .expect("operational_sol_account must be set"),
            treasury_msol_account: treasury_msol_account
                .into_inner()
                .expect("treasury_msol_account must be set"),
            min_stake: min_stake.into_inner().expect("min_stake must be set"),
            reward_fee: reward_fee.into_inner().expect("reward_fee must be set"),
            validator_manager_authority: validator_manager_authority
                .into_inner()
                .expect("validator_manager_authority msut be set"),
            validators,
            max_validators: validator_count
                + free_validator_slots
                    .into_inner()
                    .expect("free_validator_slots must be set"),
            total_cooling_down: total_cooling_down
                .into_inner()
                .expect("total_cooling_down must be set"),
            cooling_down_stakes: cooling_down_stakes
                .into_inner()
                .expect("cooling_down_stakes must be set"),
            max_stakes: stake_count
                + free_stake_slots
                    .into_inner()
                    .expect("free_stake_slots must be set"),
            liq_pool: LiqPool {
                lp_mint: lp_mint.into_inner().expect("lp_mint must be set"),
                actual_sol_amount: actual_liq_pool_sol_amount
                    .into_inner()
                    .expect("actual_liq_pool_sol_amount must be set"),
                actual_msol_amount: actual_liq_pool_msol_amount
                    .into_inner()
                    .expect("actual_liq_pool_msol_amount must be set"),
                lp_liquidity_target: lp_liquidity_target
                    .into_inner()
                    .expect("lp_liquidity_target must be set"),
                lp_max_fee: lp_max_fee.into_inner().expect("lp_max_fee must be set"),
                lp_min_fee: lp_min_fee.into_inner().expect("lp_min_fee must be set"),
                treasury_cut: lp_treasury_cut
                    .into_inner()
                    .expect("lp_treasury_cut must be set"),
                lp_supply,
                actual_lp_supply: actual_lp_supply.into_inner().unwrap_or(lp_supply),
                lent_from_sol_leg: lent_from_liq_pool.into_inner().unwrap_or(0),
            },
            available_reserve_balance,
            actual_reserve_balance: actual_reserve_balance
                .into_inner()
                .unwrap_or(available_reserve_balance + rent_exempt_for_token_acc),
            msol_supply,
            actual_msol_supply: actual_msol_supply.into_inner().unwrap_or(msol_supply),
            claim_tickets,
            slots_for_stake_delta: slots_for_stake_delta
                .into_inner()
                .expect("slots_for_stake_delta must be set"),
            last_stake_delta_epoch: last_stake_delta_epoch
                .into_inner()
                .expect("last_stake_delta_epoch must be set"),
            lent_from_reserve: lent_from_reserve.into_inner().unwrap_or(0),
            min_deposit: min_deposit.into_inner().expect("min_deposit must be set"),
            min_withdraw: min_withdraw.into_inner().expect("min_withdraw must be set"),
            staking_sol_cap: staking_sol_cap.into_inner().unwrap_or(std::u64::MAX),
            liquidity_sol_cap: liquidity_sol_cap.into_inner().unwrap_or(std::u64::MAX),
        }
    }
}
