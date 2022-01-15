/// Instruction Construction helpers for TransactionBuilder
/// one for each marinade instruction
/// (also used during tests)
///
pub mod initialize;

use std::sync::Arc;

use crate::transaction_builder::TransactionBuilder;
use log::error;
use marinade_finance_onchain_sdk::{
    marinade_finance::{located::Located, *},
    *,
};
use solana_offchain_common::solana_sdk::{pubkey::Pubkey, signer::Signer};
use thiserror::Error;

use self::initialize::InitializeBuilder;

pub trait InstructionHelpers {
    fn add_liquidity(
        &mut self,
        state: &impl Located<State>,
        transfer_from: Arc<dyn Signer>,
        mint_to: Pubkey,
        lamports: u64,
    );

    fn add_validator(
        &mut self,
        state: &impl Located<State>,
        validator_manager_authority: Arc<dyn Signer>,
        validator_vote: Pubkey,
        score: u32,
        rent_payer: Arc<dyn Signer>,
    ) -> Result<(), InstructionError>;

    fn change_authority(
        &mut self,
        state: &impl Located<State>,
        admin_authority: Arc<dyn Signer>,
        params: ChangeAuthorityData,
    ) -> Result<(), InstructionError>;

    fn deactivate_stake(
        &mut self,
        state: &impl Located<State>,
        stake_account: Pubkey,
        split_stake_account: Arc<dyn Signer>,
        split_stake_rent_payer: Arc<dyn Signer>,
        stake_index: u32,
        validator_index: u32,
    );

    fn deposit(
        &mut self,
        state: &impl Located<State>,
        transfer_from: Arc<dyn Signer>,
        mint_to: Pubkey,
        lamports: u64,
    );

    fn deposit_stake_account(
        &mut self,
        state: &impl Located<State>,
        stake_account: Pubkey,
        stake_authority: Arc<dyn Signer>,
        mint_to: Pubkey,
        validator_index: u32,
        validator_vote: Pubkey,
        rent_payer: Arc<dyn Signer>,
    );

    fn emergency_unstake(
        &mut self,
        state: &impl Located<State>,
        validator_manager_authority: Arc<dyn Signer>,
        stake_account: Pubkey,
        stake_index: u32,
        validator_index: u32,
    ) -> Result<(), InstructionError>;

    fn initialize(
        self,
        state: Arc<dyn Signer>,
        creator_authority: Arc<dyn Signer>,
    ) -> anyhow::Result<InitializeBuilder>;

    fn liquid_unstake(
        &mut self,
        state: &impl Located<State>,
        get_msol_from: Pubkey,
        get_msol_from_authority: Arc<dyn Signer>,
        transfer_sol_to: Pubkey,
        msol_amount: u64,
    );

    fn merge_stakes(
        &mut self,
        state: &impl Located<State>,
        destination_stake: Pubkey,
        destination_stake_index: u32,
        source_stake: Pubkey,
        source_stake_index: u32,
        validator_index: u32,
    );

    fn remove_liquidity(
        &mut self,
        state: &impl Located<State>,
        burn_from: Pubkey,
        burn_from_authority: Arc<dyn Signer>,
        transfer_sol_to: Pubkey,
        transfer_msol_to: Pubkey,
        tokens: u64,
    );

    fn remove_validator(
        &mut self,
        state: &impl Located<State>,
        validator_manager_authority: Arc<dyn Signer>,
        index: u32,
        validator_vote: Pubkey,
    ) -> Result<(), InstructionError>;

    fn set_lp_params(
        &mut self,
        state: &impl Located<State>,
        admin_authority: Arc<dyn Signer>,
        min_fee: Fee,
        max_fee: Fee,
        liquidity_target: u64,
    ) -> Result<(), InstructionError>;

    fn config_marinade(
        &mut self,
        state: &impl Located<State>,
        admin_authority: Arc<dyn Signer>,
        params: ConfigMarinadeParams,
    ) -> Result<(), InstructionError>;

    fn set_validator_score(
        &mut self,
        state: &impl Located<State>,
        validator_manager_authority: Arc<dyn Signer>,
        index: u32,
        validator_vote: Pubkey,
        score: u32,
    ) -> Result<(), InstructionError>;

    fn config_validator_system(
        &mut self,
        state: &impl Located<State>,
        validator_manager_authority: Arc<dyn Signer>,
        extra_runs: u32,
    ) -> Result<(), InstructionError>;

    fn stake_reserve(
        &mut self,
        state: &impl Located<State>,
        validator_index: u32,
        validator_vote: Pubkey,
        stake_account: Pubkey,
    );

    fn update_active(
        &mut self,
        state: &impl Located<State>,
        stake_account: Pubkey,
        stake_index: u32,
        validator_index: u32,
    );

    fn update_deactivated(
        &mut self,
        state: &impl Located<State>,
        stake_account: Pubkey,
        stake_index: u32,
    );

    /* TODO:
    fn update_cooling_down(
        &mut self,
        state: &impl Located<State>,
        stake_account: Pubkey,
        stake_index: u32,
        withdraw_amount: u64,
    );
    */

    fn order_unstake(
        &mut self,
        state: &impl Located<State>,
        burn_msol_from: Pubkey,
        burn_msol_from_authority: Arc<dyn Signer>,
        msol_amount: u64,
        ticket_account: Pubkey,
        // TODO: beneficiary: Pubkey,
    );

    fn claim(&mut self, state: &impl Located<State>, ticket_account: Pubkey, beneficiary: Pubkey);
}

impl InstructionHelpers for TransactionBuilder {
    fn add_liquidity(
        &mut self,
        state: &impl Located<State>,
        transfer_from: Arc<dyn Signer>,
        mint_to: Pubkey,
        lamports: u64,
    ) {
        let transfer_from = self.add_signer(transfer_from);
        self.add_instruction(
            add_liquidity(state, transfer_from, mint_to, lamports),
            format!(
                "Add {} lamports liquidity to marinade {}",
                lamports,
                state.key()
            ),
        )
        .unwrap();
    }

    fn add_validator(
        &mut self,
        state: &impl Located<State>,
        validator_manager_authority: Arc<dyn Signer>,
        validator_vote: Pubkey,
        score: u32,
        rent_payer: Arc<dyn Signer>,
    ) -> Result<(), InstructionError> {
        if validator_manager_authority.pubkey() != state.as_ref().validator_system.manager_authority
        {
            error!(
                "Validator manager authority not match. Expected {} got {}",
                state.as_ref().validator_system.manager_authority,
                validator_manager_authority.pubkey()
            );
            return Err(InstructionError::InvalidValidatorManagerAuthority {
                expected: state.as_ref().validator_system.manager_authority,
                got: validator_manager_authority.pubkey(),
            });
        }
        self.add_signer(validator_manager_authority);
        let rent_payer = self.add_signer(rent_payer);
        self.add_instruction(
            add_validator(state, validator_vote, score, rent_payer),
            format!(
                "Add validator {} into marinade {} ",
                validator_vote,
                state.key(),
            ),
        )
        .unwrap();
        Ok(())
    }

    fn change_authority(
        &mut self,
        state: &impl Located<State>,
        admin_authority: Arc<dyn Signer>,
        data: ChangeAuthorityData,
    ) -> Result<(), InstructionError> {
        if admin_authority.pubkey() != state.as_ref().admin_authority {
            error!(
                "Invalid admin authority. Expected {} got {}",
                state.as_ref().admin_authority,
                admin_authority.pubkey()
            );
            return Err(InstructionError::InvalidAdminAuthority {
                expected: state.as_ref().admin_authority,
                got: admin_authority.pubkey(),
            });
        }

        self.add_signer(admin_authority);
        self.add_instruction(
            change_authority(state, data),
            format!("Change authority for marinade instance {}", state.key()),
        )
        .unwrap();
        Ok(())
    }

    fn deactivate_stake(
        &mut self,
        state: &impl Located<State>,
        stake_account: Pubkey,
        split_stake_account: Arc<dyn Signer>,
        split_stake_rent_payer: Arc<dyn Signer>,
        stake_index: u32,
        validator_index: u32,
    ) {
        let split_stake_account = self.add_signer(split_stake_account);
        let split_stake_rent_payer = self.add_signer(split_stake_rent_payer);
        self.add_instruction(
            deactivate_stake(
                state,
                stake_account,
                split_stake_account,
                split_stake_rent_payer,
                stake_index,
                validator_index,
            ),
            format!(
                "Deactivate stake {} for marinade {}",
                stake_account,
                state.key()
            ),
        )
        .unwrap()
    }

    fn deposit(
        &mut self,
        state: &impl Located<State>,
        transfer_from: Arc<dyn Signer>,
        mint_to: Pubkey,
        lamports: u64,
    ) {
        let transfer_from = self.add_signer(transfer_from);
        self.add_instruction(
            deposit(state, transfer_from, mint_to, lamports),
            format!(
                "Deposit into marinade {} {} lamports from {} mint to {}",
                state.key(),
                lamports,
                transfer_from,
                mint_to
            ),
        )
        .unwrap();
    }

    fn deposit_stake_account(
        &mut self,
        state: &impl Located<State>,
        stake_account: Pubkey,
        stake_authority: Arc<dyn Signer>,
        mint_to: Pubkey,
        validator_index: u32,
        validator_vote: Pubkey,
        rent_payer: Arc<dyn Signer>,
    ) {
        let stake_authority = self.add_signer(stake_authority);
        let rent_payer = self.add_signer(rent_payer);
        self.add_instruction(
            deposit_stake_account(
                state,
                stake_account,
                stake_authority,
                mint_to,
                validator_index,
                validator_vote,
                rent_payer,
            ),
            format!(
                "Deposit stake account {} into marinade {}",
                stake_account,
                state.key(),
            ),
        )
        .unwrap();
    }

    fn emergency_unstake(
        &mut self,
        state: &impl Located<State>,
        validator_manager_authority: Arc<dyn Signer>,
        stake_account: Pubkey,
        stake_index: u32,
        validator_index: u32,
    ) -> Result<(), InstructionError> {
        if validator_manager_authority.pubkey() != state.as_ref().validator_system.manager_authority
        {
            error!(
                "Validator manager authority not match. Expected {} got {}",
                state.as_ref().validator_system.manager_authority,
                validator_manager_authority.pubkey()
            );
            return Err(InstructionError::InvalidValidatorManagerAuthority {
                expected: state.as_ref().validator_system.manager_authority,
                got: validator_manager_authority.pubkey(),
            });
        }
        self.add_signer(validator_manager_authority);

        self.add_instruction(
            emergency_unstake(state, stake_account, stake_index, validator_index),
            format!(
                "Emergency unstake {} from marinade {}",
                stake_account,
                state.key()
            ),
        )
        .unwrap();

        Ok(())
    }

    fn initialize(
        self,
        state: Arc<dyn Signer>,
        creator_authority: Arc<dyn Signer>,
    ) -> anyhow::Result<InitializeBuilder> {
        InitializeBuilder::new(self, state, creator_authority)
    }

    fn liquid_unstake(
        &mut self,
        state: &impl Located<State>,
        get_msol_from: Pubkey,
        get_msol_from_authority: Arc<dyn Signer>,
        transfer_sol_to: Pubkey,
        msol_amount: u64,
    ) {
        let get_msol_from_authority = self.add_signer(get_msol_from_authority);
        self.add_instruction(
            liquid_unstake(
                state,
                get_msol_from,
                get_msol_from_authority,
                transfer_sol_to,
                msol_amount,
            ),
            format!("Liquid unstake from marinade {}", state.key()),
        )
        .unwrap();
    }

    fn merge_stakes(
        &mut self,
        state: &impl Located<State>,
        destination_stake: Pubkey,
        destination_stake_index: u32,
        source_stake: Pubkey,
        source_stake_index: u32,
        validator_index: u32,
    ) {
        self.add_instruction(
            merge_stakes(
                state,
                destination_stake,
                destination_stake_index,
                source_stake,
                source_stake_index,
                validator_index,
            ),
            format!("Merge marinade {} stakes", state.key()),
        )
        .unwrap();
    }

    fn remove_liquidity(
        &mut self,
        state: &impl Located<State>,
        burn_from: Pubkey,
        burn_from_authority: Arc<dyn Signer>,
        transfer_sol_to: Pubkey,
        transfer_msol_to: Pubkey,
        tokens: u64,
    ) {
        let burn_from_authority = self.add_signer(burn_from_authority);
        self.add_instruction(
            remove_liquidity(
                state,
                burn_from,
                burn_from_authority,
                transfer_sol_to,
                transfer_msol_to,
                tokens,
            ),
            format!("Remove {} liquidity from marinade {}", tokens, state.key(),),
        )
        .unwrap();
    }

    fn remove_validator(
        &mut self,
        state: &impl Located<State>,
        validator_manager_authority: Arc<dyn Signer>,
        index: u32,
        validator_vote: Pubkey,
    ) -> Result<(), InstructionError> {
        if validator_manager_authority.pubkey() != state.as_ref().validator_system.manager_authority
        {
            error!(
                "Validator manager authority not match. Expected {} got {}",
                state.as_ref().validator_system.manager_authority,
                validator_manager_authority.pubkey()
            );
            return Err(InstructionError::InvalidValidatorManagerAuthority {
                expected: state.as_ref().validator_system.manager_authority,
                got: validator_manager_authority.pubkey(),
            });
        }
        self.add_signer(validator_manager_authority);
        self.add_instruction(
            remove_validator(state, index, validator_vote),
            format!(
                "Remove validator #{} {} from marinade {} ",
                index,
                validator_vote,
                state.key(),
            ),
        )
        .unwrap();
        Ok(())
    }

    fn set_lp_params(
        &mut self,
        state: &impl Located<State>,
        admin_authority: Arc<dyn Signer>,
        min_fee: Fee,
        max_fee: Fee,
        liquidity_target: u64,
    ) -> Result<(), InstructionError> {
        if admin_authority.pubkey() != state.as_ref().admin_authority {
            error!(
                "Admin authority not match. Expected {} got {}",
                state.as_ref().admin_authority,
                admin_authority.pubkey()
            );
            return Err(InstructionError::InvalidAdminAuthority {
                expected: state.as_ref().admin_authority,
                got: admin_authority.pubkey(),
            });
        }

        self.add_signer(admin_authority);
        self.add_instruction(
            set_lp_params(state, min_fee, max_fee, liquidity_target),
            format!("Set lp params to marinade {}", state.key()),
        )
        .unwrap();
        Ok(())
    }

    fn config_marinade(
        &mut self,
        state: &impl Located<State>,
        admin_authority: Arc<dyn Signer>,
        params: ConfigMarinadeParams,
    ) -> Result<(), InstructionError> {
        if admin_authority.pubkey() != state.as_ref().admin_authority {
            error!(
                "Invalid admin authority. Expected {} got {}",
                state.as_ref().admin_authority,
                admin_authority.pubkey()
            );
            return Err(InstructionError::InvalidAdminAuthority {
                expected: state.as_ref().admin_authority,
                got: admin_authority.pubkey(),
            });
        }

        self.add_signer(admin_authority);
        self.add_instruction(
            config_marinade(state, params),
            format!("Config marinade instance {}", state.key()),
        )
        .unwrap();
        Ok(())
    }

    fn set_validator_score(
        &mut self,
        state: &impl Located<State>,
        validator_manager_authority: Arc<dyn Signer>,
        index: u32,
        validator_vote: Pubkey,
        score: u32,
    ) -> Result<(), InstructionError> {
        if validator_manager_authority.pubkey() != state.as_ref().validator_system.manager_authority
        {
            error!(
                "Validator manager authority not match. Expected {} got {}",
                state.as_ref().validator_system.manager_authority,
                validator_manager_authority.pubkey()
            );
            return Err(InstructionError::InvalidValidatorManagerAuthority {
                expected: state.as_ref().validator_system.manager_authority,
                got: validator_manager_authority.pubkey(),
            });
        }
        self.add_signer(validator_manager_authority);
        self.add_instruction(
            set_validator_score(state, index, validator_vote, score),
            format!(
                "Set score of validator #{} {} from marinade {} to {}",
                index,
                validator_vote,
                state.key(),
                score
            ),
        )
        .unwrap();
        Ok(())
    }

    fn config_validator_system(
        &mut self,
        state: &impl Located<State>,
        validator_manager_authority: Arc<dyn Signer>,
        extra_runs: u32,
    ) -> Result<(), InstructionError> {
        if validator_manager_authority.pubkey() != state.as_ref().validator_system.manager_authority
        {
            error!(
                "Validator manager authority not match. Expected {} got {}",
                state.as_ref().validator_system.manager_authority,
                validator_manager_authority.pubkey()
            );
            return Err(InstructionError::InvalidValidatorManagerAuthority {
                expected: state.as_ref().validator_system.manager_authority,
                got: validator_manager_authority.pubkey(),
            });
        }
        self.add_signer(validator_manager_authority);
        self.add_instruction(
            config_validator_system(state, extra_runs),
            format!("Set extra_runs to {}", extra_runs),
        )
        .unwrap();
        Ok(())
    }

    fn stake_reserve(
        &mut self,
        state: &impl Located<State>,
        validator_index: u32,
        validator_vote: Pubkey,
        stake_account: Pubkey,
    ) {
        self.add_instruction(
            stake_reserve(state, validator_index, validator_vote, stake_account),
            format!("Stake reserve for marinade {}", state.key()),
        )
        .unwrap();
    }

    fn update_active(
        &mut self,
        state: &impl Located<State>,
        stake_account: Pubkey,
        stake_index: u32,
        validator_index: u32,
    ) {
        self.add_instruction(
            update_active(state, stake_account, stake_index, validator_index),
            format!(
                "Update active stake {} for marinade {}",
                stake_account,
                state.key()
            ),
        )
        .unwrap();
    }

    fn update_deactivated(
        &mut self,
        state: &impl Located<State>,
        stake_account: Pubkey,
        stake_index: u32,
    ) {
        self.add_instruction(
            update_deactivated(state, stake_account, stake_index),
            format!(
                "Update deactivated stake {} for marinade {}",
                stake_account,
                state.key()
            ),
        )
        .unwrap();
    }

    /* TODO:
    fn update_cooling_down(
        &mut self,
        state: &impl Located<State>,
        stake_account: Pubkey,
        stake_index: u32,
        withdraw_amount: u64,
    ) {
        self.add_instruction(
            update_cooling_down(state, stake_account, stake_index, withdraw_amount),
            format!(
                "Update cooling down stake {} for marinade {}",
                stake_account,
                state.key()
            ),
        )
        .unwrap();
    }
    */

    fn order_unstake(
        &mut self,
        state: &impl Located<State>,
        burn_msol_from: Pubkey,
        burn_msol_authority: Arc<dyn Signer>,
        msol_amount: u64,
        ticket_account: Pubkey,
        // TODO: beneficiary: Pubkey,
    ) {
        let burn_msol_authority = self.add_signer(burn_msol_authority);
        self.add_instruction(
            order_unstake(
                state,
                burn_msol_from,
                burn_msol_authority,
                msol_amount,
                ticket_account,
                // beneficiary,
            ),
            format!(
                "Order unstake {} mSOL from marinade {}",
                msol_amount,
                state.key()
            ),
        )
        .unwrap();
    }

    fn claim(&mut self, state: &impl Located<State>, ticket_account: Pubkey, beneficiary: Pubkey) {
        self.add_instruction(
            marinade_finance_onchain_sdk::claim(state, ticket_account, beneficiary),
            "claim".into(),
        )
        .unwrap()
    }
}

#[derive(Debug, Clone, Error)]
pub enum InstructionError {
    #[error("Validator manager authority not match. Expected {expected} got {got}")]
    InvalidValidatorManagerAuthority { expected: Pubkey, got: Pubkey },
    #[error("Admin authority not match. Expected {expected} got {got}")]
    InvalidAdminAuthority { expected: Pubkey, got: Pubkey },
}
