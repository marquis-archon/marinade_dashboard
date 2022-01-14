use std::sync::Arc;

use crate::transaction_builder::TransactionBuilder;
use anyhow::{anyhow, bail};
use log::error;
use marinade_finance_onchain_sdk::marinade_finance::{
    liq_pool::LiqPool, stake_system::StakeSystem, validator_system::ValidatorSystem, Fee,
    Initialize, State, ID,
};
use marinade_finance_onchain_sdk::{initialize, InitializeInput};
use once_cell::unsync::OnceCell;
use solana_offchain_common::solana_sdk::{
    native_token::LAMPORTS_PER_SOL, program_pack::Pack, pubkey::Pubkey, rent::Rent, signer::Signer,
};
use solana_offchain_common::spl_associated_token_account;
use spl_token::state::Mint;

#[derive(Debug)]
pub struct InitializeBuilder {
    transaction: TransactionBuilder,
    /// state_account pubkey
    pub state: Pubkey,
    pub msol_mint: OnceCell<Pubkey>,
    pub admin_authority: OnceCell<Pubkey>,
    pub operational_sol_account: OnceCell<Pubkey>,
    pub validator_manager_authority: OnceCell<Pubkey>,
    pub stake_list: OnceCell<Pubkey>,
    pub validator_list: OnceCell<Pubkey>,
    pub min_stake: OnceCell<u64>,
    pub reward_fee: OnceCell<Fee>,
    pub reserve_initialized: bool,
    // pub treasury_sol_account: OnceCell<Pubkey>,
    pub treasury_msol_account: OnceCell<Pubkey>,

    // Liq pool
    pub lp_mint: OnceCell<Pubkey>,
    pub liq_pool_sol_leg_initialized: bool,
    pub liq_pool_msol_leg: OnceCell<Pubkey>,
    pub lp_liquidity_target: OnceCell<u64>,
    pub lp_max_fee: OnceCell<Fee>,
    pub lp_min_fee: OnceCell<Fee>,
    pub lp_treasury_cut: OnceCell<Fee>,

    pub additional_state_space: OnceCell<usize>,
    pub additional_stake_record_space: u32,
    pub additional_validator_record_space: u32,
    pub slots_for_stake_delta: u64,
}

impl InitializeBuilder {
    pub fn new(
        mut transaction: TransactionBuilder,
        state: Arc<dyn Signer>,
        creator_authority: Arc<dyn Signer>,
    ) -> anyhow::Result<Self> {
        if creator_authority.pubkey() != Initialize::CREATOR_AUTHORITY {
            bail!(
                "Wrong creator authority {}. Expected {}",
                creator_authority.pubkey(),
                Initialize::CREATOR_AUTHORITY
            );
        }
        transaction.add_signer(creator_authority);
        let state = transaction.add_signer(state);
        Ok(Self {
            transaction,
            state,
            msol_mint: OnceCell::new(),
            admin_authority: OnceCell::new(),
            operational_sol_account: OnceCell::new(),
            validator_manager_authority: OnceCell::new(),
            stake_list: OnceCell::new(),
            validator_list: OnceCell::new(),
            min_stake: OnceCell::new(),
            reward_fee: OnceCell::new(),
            reserve_initialized: false,
            // treasury_sol_account: OnceCell::new(),
            treasury_msol_account: OnceCell::new(),
            lp_mint: OnceCell::new(),
            liq_pool_sol_leg_initialized: false,
            liq_pool_msol_leg: OnceCell::new(),
            lp_liquidity_target: OnceCell::new(),
            lp_max_fee: OnceCell::new(),
            lp_min_fee: OnceCell::new(),
            lp_treasury_cut: OnceCell::new(),
            additional_state_space: OnceCell::new(),
            additional_stake_record_space: 8,
            additional_validator_record_space: 8,
            // stake-delta window: how many slots counting from the end of the epoch
            // is stake-delta allowed. This is to avoid attacks making the system less efficient
            // Stake-delta should be executed near epoch's end to maximize stake/unstake orders clearing
            // 24_000 is about 4 hours, ~ the last 5% of the epoch
            slots_for_stake_delta: 24_000,
        })
    }

    pub fn state_signer(&self) -> Arc<dyn Signer> {
        self.transaction.get_signer(&self.state).unwrap()
    }

    pub fn msol_mint_authority(&self) -> Pubkey {
        State::find_msol_mint_authority(&self.state).0
    }

    pub fn use_msol_mint_pubkey(&mut self, mint: Pubkey) {
        self.msol_mint
            .set(mint)
            .expect("double mSOL mint set calls");
    }

    pub fn use_msol_mint(
        &mut self,
        mint_pubkey: Pubkey,
        mint: &Mint,
        mint_owner: Option<Arc<dyn Signer>>,
    ) -> anyhow::Result<()> {
        if self.msol_mint.get().is_some() {
            panic!("double mSOL mint set calls");
        }
        let mint_authority = mint
            .mint_authority
            .ok_or_else(|| anyhow!("mSOL mint {} must have mint authority", mint_pubkey))?;

        if mint.freeze_authority.is_some() {
            bail!(
                "Freeze authority of mSOL mint {} must not be set",
                mint_pubkey
            );
        }

        if mint.supply > 0 {
            bail!("mSOL mint {} must have 0 supply", mint_pubkey);
        }

        if mint_authority != self.msol_mint_authority() {
            // Move mint ownership
            let mint_owner = mint_owner.ok_or_else(|| {
                anyhow!(
                    "Provide mSOL mint authority {} keypair for mSOL mint account {}",
                    mint_authority,
                    mint_pubkey
                )
            })?;

            if mint_owner.pubkey() != mint_authority {
                bail!(
                    "Wrong mSOL mint owner {}. Expected {}",
                    mint_owner.pubkey(),
                    mint_authority
                );
            }

            self.transaction.add_instruction(
                spl_token::instruction::set_authority(
                    &spl_token::ID,
                    &mint_pubkey,
                    Some(&self.msol_mint_authority()),
                    spl_token::instruction::AuthorityType::MintTokens,
                    &mint_authority,
                    &[],
                )?,
                format!(
                    "Move mSOL mint {} ownership to {}",
                    mint_pubkey,
                    self.msol_mint_authority()
                ),
            )?;
        }

        self.msol_mint.set(mint_pubkey).unwrap();
        Ok(())
    }

    pub fn create_msol_mint(&mut self, msol_mint: Arc<dyn Signer>, rent: &Rent) {
        self.msol_mint
            .set(msol_mint.pubkey())
            .expect("double msol_mint set calls");

        self.transaction
            .create_mint_account(msol_mint, &self.msol_mint_authority(), None, rent, "mSOL")
            .unwrap();
    }

    pub fn set_admin_authority(&mut self, admin_authority: Pubkey) {
        // TODO: maybe not require admin signature
        self.admin_authority
            .set(admin_authority)
            .expect("double admin_authority set calls");
    }

    pub fn use_validator_manager_authority(&mut self, validator_manager_authority: Pubkey) {
        self.validator_manager_authority
            .set(validator_manager_authority)
            .expect("double validator_manager_authority set calls");
    }

    pub fn use_stake_list(&mut self, stake_list: Pubkey) {
        self.stake_list
            .set(stake_list)
            .expect("double stake_list set calls");
    }

    pub fn create_stake_list(
        &mut self,
        stake_list: Arc<dyn Signer>,
        max_stake_count: u32,
        rent: &Rent,
    ) {
        self.stake_list
            .set(stake_list.pubkey())
            .expect("double stake_list set calls");
        self.transaction
            .create_account(
                stake_list,
                StakeSystem::bytes_for_list(max_stake_count, self.additional_stake_record_space)
                    as usize,
                &ID,
                rent,
                "stake list",
            )
            .unwrap();
    }

    pub fn default_stake_list_account(&self) -> Pubkey {
        State::default_stake_list_address(&self.state)
    }

    pub fn create_stake_list_with_seed(&mut self, max_stake_count: u32, rent: &Rent) -> Pubkey {
        let stake_list = self.default_stake_list_account();
        self.stake_list
            .set(stake_list)
            .expect("double stake_list set calls");
        let actual_stake_list = self
            .transaction
            .create_account_with_seed(
                self.state_signer(),
                State::STAKE_LIST_SEED,
                StakeSystem::bytes_for_list(max_stake_count, self.additional_stake_record_space)
                    as usize,
                &ID,
                rent,
                "stake list",
            )
            .unwrap();
        assert_eq!(actual_stake_list, stake_list);
        stake_list
    }

    pub fn set_operational_sol_account(&mut self, operational_sol_account: Pubkey) {
        self.operational_sol_account
            .set(operational_sol_account)
            .expect("double operational_sol_account set calls");
    }

    pub fn use_validator_list(&mut self, validator_list: Pubkey) {
        self.validator_list
            .set(validator_list)
            .expect("double validator_list set calls");
    }

    pub fn create_validator_list(
        &mut self,
        validator_list: Arc<dyn Signer>,
        max_validator_count: u32,
        rent: &Rent,
    ) {
        self.validator_list
            .set(validator_list.pubkey())
            .expect("double validator_list set calls");
        self.transaction
            .create_account(
                validator_list,
                ValidatorSystem::bytes_for_list(
                    max_validator_count,
                    self.additional_validator_record_space,
                ) as usize,
                &ID,
                rent,
                "validator list",
            )
            .unwrap();
    }

    pub fn default_validator_list_address(&self) -> Pubkey {
        State::default_validator_list_address(&self.state)
    }

    pub fn create_validator_list_with_seed(
        &mut self,
        max_validator_count: u32,
        rent: &Rent,
    ) -> Pubkey {
        let validator_list = self.default_validator_list_address();
        self.validator_list
            .set(validator_list)
            .expect("double validator_list set calls");
        let actual_validator_list = self
            .transaction
            .create_account_with_seed(
                self.state_signer(),
                State::VALIDATOR_LIST_SEED,
                ValidatorSystem::bytes_for_list(
                    max_validator_count,
                    self.additional_validator_record_space,
                ) as usize,
                &ID,
                rent,
                "validator list",
            )
            .unwrap();
        assert_eq!(actual_validator_list, validator_list);
        validator_list
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

    pub fn reserve_address(&self) -> Pubkey {
        State::find_reserve_address(&self.state).0
    }

    pub fn init_reserve(&mut self, current_balance: u64, rent: &Rent) -> anyhow::Result<()> {
        if self.reserve_initialized {
            panic!("Double reserve initialization");
        }
        let target_balance = rent.minimum_balance(spl_token::state::Account::LEN);
        if current_balance > target_balance {
            error!(
                "To high reserve balance {}/{}",
                current_balance, target_balance
            );
            bail!(
                "To high reserve balance {}/{}",
                current_balance,
                target_balance
            );
        }
        if current_balance < target_balance {
            self.transaction
                .transfer_lamports(
                    self.transaction.fee_payer_signer(),
                    &self.reserve_address(),
                    target_balance - current_balance,
                    "fee payer",
                    "reserve",
                )
                .unwrap();
        }
        self.reserve_initialized = true;
        Ok(())
    }

    pub fn assume_reserve_initialized(&mut self) {
        self.reserve_initialized = true;
    }

    pub fn lp_mint_authority(&self) -> Pubkey {
        LiqPool::find_lp_mint_authority(&self.state).0
    }

    pub fn use_lp_mint_pubkey(&mut self, lp_mint: Pubkey) {
        self.lp_mint.set(lp_mint).expect("double LP mint set calls");
    }

    pub fn use_lp_mint(
        &mut self,
        mint_pubkey: Pubkey,
        mint: &Mint,
        mint_owner: Option<Arc<dyn Signer>>,
    ) -> anyhow::Result<()> {
        if self.lp_mint.get().is_some() {
            panic!("double LP mint set calls");
        }
        let mint_authority = mint
            .mint_authority
            .ok_or_else(|| anyhow!("LP mint {} must have mint authority", mint_pubkey))?;

        if mint.freeze_authority.is_some() {
            bail!(
                "Freeze authority of LP mint {} must not be set",
                mint_pubkey
            );
        }

        if mint.supply > 0 {
            bail!("LP mint {} must have 0 supply", mint_pubkey);
        }

        if mint_authority != self.lp_mint_authority() {
            // Move mint ownership
            let mint_owner = mint_owner.ok_or_else(|| {
                anyhow!(
                    "Provide mSOL mint authority {} keypair for mSOL mint account {}",
                    mint_authority,
                    mint_pubkey
                )
            })?;

            if mint_owner.pubkey() != mint_authority {
                bail!(
                    "Wrong mSOL mint owner {}. Expected {}",
                    mint_owner.pubkey(),
                    mint_authority
                );
            }

            self.transaction.add_instruction(
                spl_token::instruction::set_authority(
                    &spl_token::ID,
                    &mint_pubkey,
                    Some(&self.lp_mint_authority()),
                    spl_token::instruction::AuthorityType::MintTokens,
                    &mint_authority,
                    &[],
                )?,
                format!(
                    "Move LP mint {} ownership to {}",
                    mint_pubkey,
                    self.lp_mint_authority()
                ),
            )?;
        }

        self.lp_mint.set(mint_pubkey).unwrap();
        Ok(())
    }

    pub fn create_lp_mint(&mut self, lp_mint: Arc<dyn Signer>, rent: &Rent) {
        self.lp_mint
            .set(lp_mint.pubkey())
            .expect("double lp_mint set calls");

        self.transaction
            .create_mint_account(lp_mint, &self.lp_mint_authority(), None, rent, "lp")
            .unwrap();
    }

    pub fn use_liq_pool_msol_leg(&mut self, msol_leg: Pubkey) {
        self.liq_pool_msol_leg
            .set(msol_leg)
            .expect("double liq_pool_msol_leg set calls");
    }

    pub fn liq_pool_msol_leg_authority(&self) -> Pubkey {
        LiqPool::find_msol_leg_authority(&self.state).0
    }

    pub fn create_liq_pool_msol_leg(&mut self, msol_leg: Arc<dyn Signer>, rent: &Rent) {
        self.liq_pool_msol_leg
            .set(msol_leg.pubkey())
            .expect("double liq_pool_msol_leg set calls");

        self.transaction
            .create_token_account(
                msol_leg,
                self.msol_mint
                    .get()
                    .expect("st sol mint must be set previously"),
                &self.liq_pool_msol_leg_authority(),
                rent,
                "liq pool mSOL leg",
            )
            .unwrap();
    }

    pub fn default_liq_pool_msol_leg_address(&self) -> Pubkey {
        LiqPool::default_msol_leg_address(&self.state)
    }

    pub fn create_liq_pool_msol_leg_with_seed(&mut self, rent: &Rent) -> Pubkey {
        let msol_leg = self.default_liq_pool_msol_leg_address();
        self.liq_pool_msol_leg
            .set(msol_leg)
            .expect("double liq pool msol_account set calls");

        let actual_msol_leg = self
            .transaction
            .create_token_account_with_seed(
                self.state_signer(),
                LiqPool::MSOL_LEG_SEED,
                self.msol_mint
                    .get()
                    .expect("msol mint must be set previously"),
                &self.liq_pool_msol_leg_authority(),
                rent,
                "liq pool mSOL leg",
            )
            .unwrap();
        assert_eq!(actual_msol_leg, msol_leg);
        msol_leg
    }

    pub fn liq_pool_sol_leg_address(&self) -> Pubkey {
        LiqPool::find_sol_leg_address(&self.state).0
    }

    pub fn init_liq_pool_sol_leg(
        &mut self,
        current_balance: u64,
        rent: &Rent,
    ) -> anyhow::Result<()> {
        if self.liq_pool_sol_leg_initialized {
            panic!("Double initialization of liq_pool_sol_leg");
        }
        let target_balance = rent.minimum_balance(spl_token::state::Account::LEN);
        if current_balance > target_balance {
            error!(
                "To high liq_pool_sol_leg balance {}/{}",
                current_balance, target_balance
            );
            bail!(
                "To high liq_pool_sol_leg balance {}/{}",
                current_balance,
                target_balance
            );
        }
        if current_balance < target_balance {
            self.transaction
                .transfer_lamports(
                    self.transaction.fee_payer_signer(),
                    &self.liq_pool_sol_leg_address(),
                    target_balance - current_balance,
                    "fee payer",
                    "liq pool SOL leg",
                )
                .unwrap();
        }
        self.liq_pool_sol_leg_initialized = true;
        Ok(())
    }

    /*
    pub fn use_treasury_sol_account(&mut self, treasury_sol_account: Pubkey) {
        self.treasury_sol_account
            .set(treasury_sol_account)
            .expect("double treasury_sol_account set calls");
    }

    pub fn init_treasury_sol_account(
        &mut self,
        treasury_sol_account: Pubkey,
        current_balance: u64,
        rent: &Rent,
    ) {
        self.treasury_sol_account
            .set(treasury_sol_account)
            .expect("double treasury_sol_account set calls");

        let target_balance = rent.minimum_balance(0);

        if current_balance < target_balance {
            self.transaction
                .transfer_lamports(
                    self.transaction.fee_payer_signer(),
                    &treasury_sol_account,
                    target_balance - current_balance,
                    "fee payer",
                    "treasury SOL account",
                )
                .unwrap();
        }
    }*/

    pub fn use_treasury_msol_account(&mut self, treasury_msol_account: Pubkey) {
        self.treasury_msol_account
            .set(treasury_msol_account)
            .expect("double treasury_msol_account set calls");
    }

    pub fn default_treasury_msol_account(&self, treasury_msol_authority: Pubkey) -> Pubkey {
        let msol_mint = self
            .msol_mint
            .get()
            .expect("msol_mint must be set previously");
        println!(
            "--treasury msol acc as spl_associated_token_account base:{} mint:{}",
            &treasury_msol_authority, msol_mint
        );
        spl_associated_token_account::get_associated_token_address(
            &treasury_msol_authority,
            msol_mint,
        )
    }

    pub fn create_treasury_msol_account(&mut self, treasury_msol_authority: Pubkey) -> Pubkey {
        let msol_mint = self
            .msol_mint
            .get()
            .expect("msol_mint must be set previously");
        let treasury_msol_account = self.default_treasury_msol_account(treasury_msol_authority);
        self.treasury_msol_account
            .set(treasury_msol_account)
            .expect("double treasury_msol_account set calls");

        let actual_treasury_msol_account = self
            .transaction
            .create_associated_token_account(&treasury_msol_authority, msol_mint, "treasury mSOL")
            .unwrap();
        assert_eq!(actual_treasury_msol_account, treasury_msol_account);
        treasury_msol_account
    }

    pub fn set_additional_state_space(&mut self, space: usize) {
        self.additional_state_space
            .set(space)
            .expect("double additional_state_space set calls")
    }

    pub fn set_slots_for_stake_delta(&mut self, slots: u64) {
        self.slots_for_stake_delta = slots
    }

    pub fn build(mut self, rent: &Rent) -> TransactionBuilder {
        let state_len =
            State::serialized_len() + *self.additional_state_space.get().unwrap_or(&2048); //&320);
        self.transaction.begin();
        self.transaction
            .create_account(self.state_signer(), state_len, &ID, rent, "marinade state")
            .unwrap();
        self.transaction
            .add_instruction(
                initialize(InitializeInput {
                    state: self.state,
                    stake_list: self
                        .stake_list
                        .into_inner()
                        .expect("stake_list must be set"),
                    validator_list: self
                        .validator_list
                        .into_inner()
                        .expect("validator_list must be set"),
                    msol_mint: self.msol_mint.into_inner().expect("msol_mint must be set"),
                    admin_authority: self
                        .admin_authority
                        .into_inner()
                        .expect("admin_authority must be set"),
                    operational_sol_account: self
                        .operational_sol_account
                        .into_inner()
                        .expect("operational_sol_account must be set"),
                    validator_manager_authority: self
                        .validator_manager_authority
                        .into_inner()
                        .expect("validator_manager_authority must be set"),
                    /*treasury_sol_account: self
                    .treasury_sol_account
                    .into_inner()
                    .expect("treasury_sol_account must be set"), */
                    treasury_msol_account: self
                        .treasury_msol_account
                        .into_inner()
                        .expect("treasury_msol_account must be set"),
                    lp_mint: self.lp_mint.into_inner().expect("lp_mint must be set"),
                    liq_pool_msol_leg: self
                        .liq_pool_msol_leg
                        .into_inner()
                        .expect("liq pool msol_account must be set"),
                    min_stake: *self.min_stake.get().unwrap_or(&LAMPORTS_PER_SOL), // default min_stake is 1 SOL
                    reward_fee: self
                        .reward_fee
                        .into_inner()
                        .expect("reward_fee must be set"),
                    lp_liquidity_target: self
                        .lp_liquidity_target
                        .into_inner()
                        .unwrap_or(10_000 * LAMPORTS_PER_SOL), // 10_000 SOL,
                    lp_max_fee: self
                        .lp_max_fee
                        .into_inner()
                        .unwrap_or_else(|| Fee::from_basis_points(300)), // 3%,
                    lp_min_fee: self
                        .lp_min_fee
                        .into_inner()
                        .unwrap_or_else(|| Fee::from_basis_points(30)),
                    lp_treasury_cut: self
                        .lp_treasury_cut
                        .into_inner()
                        .unwrap_or_else(|| Fee::from_basis_points(2500)), // 25%,
                    additional_stake_record_space: self.additional_stake_record_space,
                    additional_validator_record_space: self.additional_validator_record_space,
                    slots_for_stake_delta: self.slots_for_stake_delta,
                }),
                format!("Init marinade state {}", self.state),
            )
            .unwrap();
        self.transaction.commit();
        self.transaction
    }
}
