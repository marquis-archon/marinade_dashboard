use crate::integration_test::IntegrationTest;
use assert_json_diff::assert_json_eq;
use log::info;
use marinade_finance_offchain_sdk::anchor_lang::prelude::Clock;
use marinade_finance_offchain_sdk::anchor_lang::solana_program::{
    clock::Epoch,
    native_token::LAMPORTS_PER_SOL,
    program_pack::Pack,
    stake::state::{Authorized, Delegation, Lockup, Meta, Stake},
};
use marinade_finance_offchain_sdk::{
    instruction_helpers::InstructionHelpers,
    marinade_finance::{Fee, State},
    transaction_builder::TransactionBuilder,
    WithKey,
};
use marinade_reflection::{
    accounts_builder::AccountsBuilder,
    builder::RandomBuildParams,
    marinade::{Marinade, Validator},
    random_pubkey,
};
use rand::{distributions::Uniform, prelude::*, SeedableRng};
use rand_chacha::ChaChaRng;
use solana_sdk::{
    account_info::AccountInfo, entrypoint::ProgramResult, instruction::Instruction,
    stake::state::StakeState, sysvar::rent::Rent,
};
use solana_vote_program::vote_state::{VoteInit, VoteState};
use std::{collections::HashMap, sync::Arc};
use test_env_log::test;

#[test(tokio::test)]
async fn test_update_active() -> anyhow::Result<()> {
    use rand_chacha::rand_core::SeedableRng;
    let mut rng = ChaChaRng::from_seed([
        185, 142, 125, 189, 226, 140, 157, 209, 91, 138, 122, 151, 98, 172, 99, 218, 41, 143, 127,
        248, 11, 124, 221, 113, 24, 23, 118, 147, 37, 190, 165, 38,
    ]);
    let rent = Rent::default(); // must be the equal to actual rent sysvar in blockchain. Will be checked later

    let mut builder = marinade_reflection::builder::Builder::default();
    builder.set_reward_fee(Fee::from_basis_points(0)); // Do not change mSOL supply for simplicity
    let params = RandomBuildParams::pick(&mut builder, &mut rng);
    builder.random_fill(&mut rng, &params, &rent); // basic state

    // Validator for test
    let (validator_vote, validator) = builder
        .validators
        .iter_mut()
        .find(|(_key, validator)| validator.stake_count > 0)
        .expect("There must be non empty validator");
    let validator_vote = *validator_vote;

    let last_update_delegated_lamports = 11 * LAMPORTS_PER_SOL + 2343;
    let actual_delegation = 12 * LAMPORTS_PER_SOL + 10267;
    let extra_balance = 0; // for not changing mSOL supply
    let test_stake = marinade_reflection::accounts_builder::StakeBuilder {
        address: random_pubkey(&mut rng),
        voter_pubkey: validator_vote,
        stake: actual_delegation,
        is_active: true,
        last_update_delegated_lamports,
        last_update_epoch: 0,
        extra_balance,
    };
    // Add test stake
    validator.stake_count += 1;
    validator.total_delegated_delta += actual_delegation - last_update_delegated_lamports;
    validator.total_extra_balance += extra_balance;

    let initial_reflection = builder.build(&rent);
    // It must be unbalanced by random generator
    assert_ne!(
        initial_reflection.actual_msol_supply,
        initial_reflection.msol_supply
    );
    assert_ne!(
        initial_reflection.available_reserve_balance + StakeState::get_rent_exempt_reserve(&rent),
        initial_reflection.actual_reserve_balance
    );

    let mut account_builder = AccountsBuilder::new_random(&initial_reflection, &mut rng, 0, 0);
    let clock = Clock::default(); // TODO?

    // Install test validator
    let validator_identity = random_pubkey(&mut rng);
    account_builder.add_validator(
        validator_vote,
        VoteState::new(
            &VoteInit {
                node_pubkey: validator_identity,
                authorized_voter: validator_identity,
                ..VoteInit::default()
            },
            &clock,
        ),
    )?;

    account_builder.add_stake(test_stake.clone())?;

    // Install all other validators
    account_builder.random_fill(&mut rng);

    let mut test =
        IntegrationTest::start_synthetic(&account_builder, HashMap::new(), &mut rng).await?;

    let stake_index = account_builder
        .stakes
        .iter()
        .position(|stake| stake.address == test_stake.address)
        .unwrap() as u32;
    let validator_index = account_builder
        .validators
        .iter()
        .position(|validator| validator.vote_address == validator_vote)
        .unwrap() as u32;
    test.builder.update_active(
        &test.state,
        test_stake.address,
        stake_index,
        validator_index,
    );
    test.execute().await;

    let mut expected_reflection = initial_reflection.clone();
    // It must send extra balance to reserve
    expected_reflection.actual_reserve_balance += extra_balance;
    // And update available_reserve_balance field
    expected_reflection.available_reserve_balance = expected_reflection.actual_reserve_balance
        - rent.minimum_balance(spl_token::state::Account::LEN);
    // It must update msol_supply
    expected_reflection.msol_supply = initial_reflection.actual_msol_supply;
    let validator_reflection = expected_reflection
        .validators
        .get_mut(&validator_vote)
        .expect("Validator not found");
    // count acount delegation instead of last update
    validator_reflection.active_balance += test_stake.stake;
    validator_reflection.active_balance -= test_stake.last_update_delegated_lamports;
    validator_reflection.total_delegated_delta += test_stake.last_update_delegated_lamports;
    validator_reflection.total_delegated_delta -= test_stake.stake;
    validator_reflection.total_extra_balance -= test_stake.extra_balance;

    assert_json_eq!(test.reflection, expected_reflection);
    /*let result_stake_account = banks_client
        .get_account(test_stake.address)
        .await
        .unwrap()
        .unwrap();
    let result_stake: StakeState = bincode::deserialize(&result_stake_account.data).unwrap();
    assert_eq!(
        result_stake.delegation().unwrap().stake + result_stake.meta().unwrap().rent_exempt_reserve,
        result_stake_account.lamports
    );*/

    Ok(())
}
