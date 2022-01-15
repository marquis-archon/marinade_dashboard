#![cfg_attr(not(debug_assertions), deny(warnings))]

//! Marinade Program SDK
pub use ::marinade_finance; // reexport contract crate

use ::marinade_finance::*;
use anchor_lang::solana_program::{
    instruction::Instruction,
    pubkey::Pubkey,
    stake, system_program,
    sysvar::{clock, epoch_schedule, rent, stake_history},
};
/// instruction helpers to be used by:
/// * other on-chain programs
/// * cli tools
/// * integration tests
use anchor_lang::{InstructionData, ToAccountMetas};
use anchor_spl::token;
use marinade_finance::{
    liq_pool::{LiqPool, LiqPoolHelpers},
    located::Located,
    stake_system::StakeSystemHelpers,
    state::StateHelpers,
    validator_system::ValidatorRecord,
};

pub fn add_liquidity(
    state: &impl Located<State>,
    transfer_from: Pubkey,
    mint_to: Pubkey,
    lamports: u64,
) -> Instruction {
    let accounts = accounts::AddLiquidity {
        state: state.key(),
        lp_mint: state.as_ref().liq_pool.lp_mint,
        lp_mint_authority: state.lp_mint_authority(),
        // msol_mint: state.as_ref().msol_mint,
        liq_pool_msol_leg: state.as_ref().liq_pool.msol_leg,
        liq_pool_sol_leg_pda: state.liq_pool_sol_leg_address(),
        transfer_from,
        mint_to,
        system_program: system_program::ID,
        token_program: token::ID,
    }
    .to_account_metas(None);

    let data = instruction::AddLiquidity { lamports };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn add_validator(
    state: &impl Located<State>,
    validator_vote: Pubkey,
    score: u32,
    rent_payer: Pubkey,
) -> Instruction {
    let accounts = accounts::AddValidator {
        state: state.key(),
        manager_authority: state.as_ref().validator_system.manager_authority,
        validator_list: *state.as_ref().validator_system.validator_list_address(),
        validator_vote,
        duplication_flag: ValidatorRecord::find_duplication_flag(&state.key(), &validator_vote).0,
        rent_payer,
        clock: clock::ID,
        rent: rent::ID,
        system_program: system_program::ID,
    }
    .to_account_metas(None);

    let data = instruction::AddValidator { score };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn change_authority(state: &impl Located<State>, data: ChangeAuthorityData) -> Instruction {
    let accounts = accounts::ChangeAuthority {
        state: state.key(),
        admin_authority: state.as_ref().admin_authority,
    }
    .to_account_metas(None);

    let data = instruction::ChangeAuthority { data };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn deactivate_stake(
    state: &impl Located<State>,
    stake_account: Pubkey,
    split_stake_account: Pubkey,
    split_stake_rent_payer: Pubkey,
    stake_index: u32,
    validator_index: u32,
) -> Instruction {
    let accounts = accounts::DeactivateStake {
        state: state.key(),
        reserve_pda: state.reserve_address(),
        validator_list: *state.as_ref().validator_system.validator_list_address(),
        stake_list: *state.as_ref().stake_system.stake_list_address(),
        stake_account,
        stake_deposit_authority: state.stake_deposit_authority(),
        split_stake_account,
        split_stake_rent_payer,

        clock: clock::ID,
        rent: rent::ID,
        epoch_schedule: epoch_schedule::ID,
        stake_history: stake_history::id(),

        system_program: system_program::ID,
        stake_program: stake::program::ID,
    }
    .to_account_metas(None);

    let data = instruction::DeactivateStake {
        stake_index,
        validator_index,
    };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn deposit(
    state: &impl Located<State>,
    transfer_from: Pubkey,
    mint_to: Pubkey,
    lamports: u64,
) -> Instruction {
    let accounts = accounts::Deposit {
        state: state.key(),
        msol_mint: state.as_ref().msol_mint,
        liq_pool_sol_leg_pda: state.liq_pool_sol_leg_address(),
        liq_pool_msol_leg: state.as_ref().liq_pool.msol_leg,
        liq_pool_msol_leg_authority: state.liq_pool_msol_leg_authority(),
        reserve_pda: state.reserve_address(),
        transfer_from,
        mint_to,
        msol_mint_authority: state.msol_mint_authority(),
        system_program: system_program::ID,
        token_program: token::ID,
    }
    .to_account_metas(None);

    let data = instruction::Deposit { lamports };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn deposit_stake_account(
    state: &impl Located<State>,
    stake_account: Pubkey,
    stake_authority: Pubkey,
    mint_to: Pubkey,
    validator_index: u32,
    validator_vote: Pubkey,
    rent_payer: Pubkey,
) -> Instruction {
    let accounts = accounts::DepositStakeAccount {
        state: state.key(),
        validator_list: *state.as_ref().validator_system.validator_list_address(),
        stake_list: *state.as_ref().stake_system.stake_list_address(),
        stake_account,
        stake_authority,
        duplication_flag: ValidatorRecord::find_duplication_flag(&state.key(), &validator_vote).0,
        rent_payer,
        msol_mint: state.as_ref().msol_mint,
        mint_to,
        msol_mint_authority: state.msol_mint_authority(),
        clock: clock::id(),
        rent: rent::id(),
        system_program: system_program::ID,
        token_program: token::ID,
        stake_program: stake::program::ID,
    }
    .to_account_metas(None);

    let data = instruction::DepositStakeAccount { validator_index };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn emergency_unstake(
    state: &impl Located<State>,
    stake_account: Pubkey,
    stake_index: u32,
    validator_index: u32,
) -> Instruction {
    let accounts = accounts::EmergencyUnstake {
        state: state.key(),
        validator_manager_authority: state.as_ref().validator_system.manager_authority,
        validator_list: *state.as_ref().validator_system.validator_list_address(),
        stake_list: *state.as_ref().stake_system.stake_list_address(),
        stake_account,
        stake_deposit_authority: state.stake_deposit_authority(),

        clock: clock::ID,

        stake_program: stake::program::ID,
    }
    .to_account_metas(None);

    let data = instruction::EmergencyUnstake {
        stake_index,
        validator_index,
    };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub struct InitializeInput {
    pub state: Pubkey,
    pub stake_list: Pubkey,
    pub validator_list: Pubkey,
    pub msol_mint: Pubkey,
    pub admin_authority: Pubkey,
    pub operational_sol_account: Pubkey,
    pub validator_manager_authority: Pubkey,
    // pub treasury_sol_account: Pubkey,
    pub treasury_msol_account: Pubkey,
    pub lp_mint: Pubkey,
    pub liq_pool_msol_leg: Pubkey,
    pub min_stake: u64,
    pub reward_fee: Fee,
    pub lp_liquidity_target: u64,
    pub lp_max_fee: Fee,
    pub lp_min_fee: Fee,
    pub lp_treasury_cut: Fee,
    pub additional_stake_record_space: u32,
    pub additional_validator_record_space: u32,
    pub slots_for_stake_delta: u64,
}

pub fn initialize(
    InitializeInput {
        state,
        stake_list,
        validator_list,
        msol_mint,
        admin_authority,
        operational_sol_account,
        validator_manager_authority,
        // treasury_sol_account,
        treasury_msol_account,
        lp_mint,
        liq_pool_msol_leg,
        min_stake,
        reward_fee,
        lp_liquidity_target,
        lp_max_fee,
        lp_min_fee,
        lp_treasury_cut,
        additional_stake_record_space,
        additional_validator_record_space,
        slots_for_stake_delta,
    }: InitializeInput,
) -> Instruction {
    let accounts = accounts::Initialize {
        creator_authority: Initialize::CREATOR_AUTHORITY,
        state,
        reserve_pda: State::find_reserve_address(&state).0,
        stake_list,
        validator_list,
        msol_mint,
        operational_sol_account,
        // treasury_sol_account,
        treasury_msol_account,

        clock: clock::id(),
        rent: rent::id(),
        liq_pool: accounts::LiqPoolInitialize {
            lp_mint,
            sol_leg_pda: LiqPool::find_sol_leg_address(&state).0,
            msol_leg: liq_pool_msol_leg,
        },
    }
    .to_account_metas(None);

    let data = instruction::Initialize {
        data: InitializeData {
            admin_authority,
            validator_manager_authority,
            min_stake,
            reward_fee,
            additional_stake_record_space,
            additional_validator_record_space,
            slots_for_stake_delta,
            liq_pool: LiqPoolInitializeData {
                lp_liquidity_target,
                lp_max_fee,
                lp_min_fee,
                lp_treasury_cut,
            },
        },
    };
    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn liquid_unstake(
    state: &impl Located<State>,
    get_msol_from: Pubkey,
    get_msol_from_authority: Pubkey,
    transfer_sol_to: Pubkey,
    msol_amount: u64,
) -> Instruction {
    let accounts = accounts::LiquidUnstake {
        state: state.key(),
        msol_mint: state.as_ref().msol_mint,
        liq_pool_sol_leg_pda: state.liq_pool_sol_leg_address(),
        liq_pool_msol_leg: state.as_ref().liq_pool.msol_leg,
        get_msol_from,
        get_msol_from_authority,
        transfer_sol_to,
        treasury_msol_account: state.as_ref().treasury_msol_account,
        system_program: system_program::ID,
        token_program: token::ID,
    }
    .to_account_metas(None);

    let data = instruction::LiquidUnstake { msol_amount };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn merge_stakes(
    state: &impl Located<State>,
    destination_stake: Pubkey,
    destination_stake_index: u32,
    source_stake: Pubkey,
    source_stake_index: u32,
    validator_index: u32,
) -> Instruction {
    let accounts = accounts::MergeStakes {
        state: state.key(),
        stake_list: *state.as_ref().stake_system.stake_list_address(),
        validator_list: *state.as_ref().validator_system.validator_list_address(),
        destination_stake,
        source_stake,
        stake_deposit_authority: state.stake_deposit_authority(),
        stake_withdraw_authority: state.stake_withdraw_authority(),
        operational_sol_account: state.as_ref().operational_sol_account,

        clock: clock::ID,
        stake_history: stake_history::id(),

        stake_program: stake::program::ID,
    }
    .to_account_metas(None);

    let data = instruction::MergeStakes {
        destination_stake_index,
        source_stake_index,
        validator_index,
    };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn order_unstake(
    state: &impl Located<State>,
    burn_msol_from: Pubkey,
    burn_msol_authority: Pubkey, // delegated or owner
    msol_amount: u64,
    new_ticket_account: Pubkey,
) -> Instruction {
    let accounts = accounts::OrderUnstake {
        state: state.key(),
        msol_mint: state.as_ref().msol_mint,
        burn_msol_from,
        burn_msol_authority,
        new_ticket_account,
        token_program: token::ID,
        clock: clock::ID,
        rent: rent::ID,
    }
    .to_account_metas(None);

    let data = instruction::OrderUnstake { msol_amount };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn remove_liquidity(
    state: &impl Located<State>,
    burn_from: Pubkey,
    burn_from_authority: Pubkey,
    transfer_sol_to: Pubkey,
    transfer_msol_to: Pubkey,
    tokens: u64,
) -> Instruction {
    let accounts = accounts::RemoveLiquidity {
        state: state.key(),
        lp_mint: state.as_ref().liq_pool.lp_mint,
        // msol_mint: state.as_ref().msol_mint,
        burn_from,
        burn_from_authority, //owner acc is also token owner
        transfer_sol_to,
        transfer_msol_to,
        liq_pool_sol_leg_pda: state.liq_pool_sol_leg_address(),
        liq_pool_msol_leg: state.as_ref().liq_pool.msol_leg,
        liq_pool_msol_leg_authority: state.liq_pool_msol_leg_authority(),
        system_program: system_program::ID,
        token_program: token::ID,
    }
    .to_account_metas(None);

    let data = instruction::RemoveLiquidity { tokens };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn remove_validator(
    state: &impl Located<State>,
    index: u32,
    validator_vote: Pubkey,
) -> Instruction {
    let accounts = accounts::RemoveValidator {
        state: state.key(),
        manager_authority: state.as_ref().validator_system.manager_authority,
        validator_list: *state.as_ref().validator_system.validator_list_address(),
        duplication_flag: ValidatorRecord::find_duplication_flag(&state.key(), &validator_vote).0,
        operational_sol_account: state.as_ref().operational_sol_account,
    }
    .to_account_metas(None);

    let data = instruction::RemoveValidator {
        index,
        validator_vote,
    };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn set_lp_params(
    state: &impl Located<State>,
    min_fee: Fee,
    max_fee: Fee,
    liquidity_target: u64,
) -> Instruction {
    let accounts = accounts::SetLpParams {
        state: state.key(),
        admin_authority: state.as_ref().admin_authority,
    }
    .to_account_metas(None);

    let data = instruction::SetLpParams {
        min_fee,
        max_fee,
        liquidity_target,
    };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn config_marinade(state: &impl Located<State>, params: ConfigMarinadeParams) -> Instruction {
    let accounts = accounts::ConfigMarinade {
        state: state.key(),
        admin_authority: state.as_ref().admin_authority,
    }
    .to_account_metas(None);

    let data = instruction::ConfigMarinade { params };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn config_validator_system(state: &impl Located<State>, extra_runs: u32) -> Instruction {
    let accounts = accounts::ConfigValidatorSystem {
        state: state.key(),
        manager_authority: state.as_ref().validator_system.manager_authority,
    }
    .to_account_metas(None);

    let data = instruction::ConfigValidatorSystem { extra_runs };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn set_validator_score(
    state: &impl Located<State>,
    index: u32,
    validator_vote: Pubkey,
    score: u32,
) -> Instruction {
    let accounts = accounts::SetValidatorScore {
        state: state.key(),
        manager_authority: state.as_ref().validator_system.manager_authority,
        validator_list: *state.as_ref().validator_system.validator_list_address(),
    }
    .to_account_metas(None);

    let data = instruction::SetValidatorScore {
        index,
        validator_vote,
        score,
    };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn stake_reserve(
    state: &impl Located<State>,
    validator_index: u32,
    validator_vote: Pubkey,
    stake_account: Pubkey,
) -> Instruction {
    let accounts = accounts::StakeReserve {
        state: state.key(),
        validator_list: *state.as_ref().validator_system.validator_list_address(),
        stake_list: *state.as_ref().stake_system.stake_list_address(),
        validator_vote,
        reserve_pda: state.reserve_address(),
        stake_account,
        stake_deposit_authority: state.stake_deposit_authority(),
        clock: clock::ID,
        epoch_schedule: epoch_schedule::ID,
        rent: rent::ID,
        stake_history: stake_history::ID,
        stake_config: stake::config::ID,
        system_program: system_program::ID,
        stake_program: stake::program::ID,
    }
    .to_account_metas(None);

    let data = instruction::StakeReserve { validator_index };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn update_active(
    state: &impl Located<State>,
    stake_account: Pubkey,
    stake_index: u32,
    validator_index: u32,
) -> Instruction {
    let accounts = accounts::UpdateActive {
        common: accounts::UpdateCommon {
            state: state.key(),
            stake_list: *state.as_ref().stake_system.stake_list_address(),
            stake_account,
            stake_withdraw_authority: state.stake_withdraw_authority(),
            reserve_pda: state.reserve_address(),
            msol_mint: state.as_ref().msol_mint,
            clock: clock::ID,
            stake_history: stake_history::ID,
            msol_mint_authority: state.msol_mint_authority(),
            treasury_msol_account: state.as_ref().treasury_msol_account,
            token_program: token::ID,
            stake_program: stake::program::ID,
        },

        validator_list: *state.as_ref().validator_system.validator_list_address(),
    }
    .to_account_metas(None);

    let data = instruction::UpdateActive {
        stake_index,
        validator_index,
    };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

pub fn update_deactivated(
    state: &impl Located<State>,
    stake_account: Pubkey,
    stake_index: u32,
) -> Instruction {
    let accounts = accounts::UpdateDeactivated {
        common: accounts::UpdateCommon {
            state: state.key(),
            stake_list: *state.as_ref().stake_system.stake_list_address(),
            stake_account,
            stake_withdraw_authority: state.stake_withdraw_authority(),
            reserve_pda: state.reserve_address(),
            msol_mint: state.as_ref().msol_mint,
            clock: clock::ID,
            stake_history: stake_history::ID,
            msol_mint_authority: state.msol_mint_authority(),
            treasury_msol_account: state.as_ref().treasury_msol_account,
            token_program: token::ID,
            stake_program: stake::program::ID,
        },
        operational_sol_account: state.as_ref().operational_sol_account,
        system_program: system_program::ID,
    }
    .to_account_metas(None);

    let data = instruction::UpdateDeactivated { stake_index };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}

/* TODO:
pub fn update_cooling_down(
    state: &impl Located<State>,
    stake_account: Pubkey,
    stake_index: u32,
    withdraw_amount: u64,
) -> Instruction {
    let accounts = accounts::UpdateCoolingDown {
        common: accounts::UpdateCommon {
            state: state.key(),
            stake_list: *state.as_ref().stake_system.stake_list_address(),
            stake_account,
            stake_withdraw_authority: state.stake_withdraw_authority(),
            reserve_pda: state.reserve_address(),
            msol_mint: state.as_ref().msol_mint,
            clock: clock::ID,
            stake_history: stake_history::ID,
            msol_mint_authority: state.msol_mint_authority(),
            treasury_msol_account: state.as_ref().treasury_msol_account,
            token_program: spl_token::ID,
            stake_program: stake::program::ID,
        },
    }
    .to_account_metas(None);

    let data = instruction::UpdateCoolingDown {
        stake_index,
        withdraw_amount,
    };

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}*/

pub fn claim(
    state: &impl Located<State>,
    ticket_account: Pubkey,
    transfer_sol_to: Pubkey,
) -> Instruction {
    let accounts = accounts::Claim {
        state: state.key(),
        reserve_pda: state.reserve_address(),
        ticket_account,
        transfer_sol_to,
        system_program: system_program::ID,
        clock: clock::ID,
    }
    .to_account_metas(None);

    let data = instruction::Claim {};

    Instruction {
        program_id: marinade_finance::ID,
        accounts,
        data: data.data(),
    }
}
