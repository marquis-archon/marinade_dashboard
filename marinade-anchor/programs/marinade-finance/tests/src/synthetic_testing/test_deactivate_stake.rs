use crate::integration_test::IntegrationTest;
use marinade_finance_offchain_sdk::anchor_lang::prelude::Clock;
use assert_json_diff::assert_json_eq;
use log::info;
use marinade_finance_offchain_sdk::{
    instruction_helpers::InstructionHelpers, transaction_builder::TransactionBuilder, marinade_finance::{Fee, State},
    WithKey,
};
use marinade_reflection::{
    accounts_builder::AccountsBuilder,
    builder::RandomBuildParams,
    marinade::{ClaimTicket, Marinade, Validator},
    random_pubkey,
};
use rand::{distributions::Uniform, prelude::*, SeedableRng};
use rand_chacha::ChaChaRng;
use marinade_finance_offchain_sdk::anchor_lang::solana_program::{
    clock::Epoch,
    native_token::LAMPORTS_PER_SOL,
    program_pack::Pack,
    stake::{
        self,
        state::{Authorized, Delegation, Lockup, Meta, Stake},
    },
};
use solana_sdk::{
    account_info::AccountInfo, entrypoint::ProgramResult, instruction::Instruction,
    signature::Keypair, stake::state::StakeState, sysvar::rent::Rent,
};
use solana_vote_program::vote_state::{VoteInit, VoteState};
use std::{collections::HashMap, sync::Arc};
use test_env_log::test;

#[test(tokio::test)]
async fn test_deactivate_stake_whole() -> anyhow::Result<()> {
    use rand_chacha::rand_core::SeedableRng;
    let mut rng = ChaChaRng::from_seed([
        141, 96, 118, 67, 115, 241, 200, 41, 146, 242, 189, 27, 212, 218, 246, 147, 48, 170, 236,
        160, 46, 216, 124, 133, 25, 50, 0, 255, 192, 248, 68, 111,
    ]);
    let rent = Rent::default(); // must be the equal to actual rent sysvar in blockchain. Will be checked later

    let mut builder = marinade_reflection::builder::Builder::default();
    builder.set_min_stake(LAMPORTS_PER_SOL);
    builder.set_cooling_down_stakes(0);
    builder.set_total_cooling_down(0);
    builder.set_available_reserve_balance(0);
    builder.set_actual_reserve_balance(rent.minimum_balance(spl_token::state::Account::LEN));
    let params = RandomBuildParams::default();
    builder.random_fill(&mut rng, &params, &rent); // basic state

    // Validator for test
    let validator_vote = random_pubkey(&mut rng);
    let stake_delegation = 20 * LAMPORTS_PER_SOL;
    builder.add_validator(
        validator_vote,
        Validator {
            active_balance: stake_delegation,
            stake_count: 1,
            score: Uniform::from(1..200).sample(&mut rng),
            last_stake_delta_epoch: Epoch::MAX,
            total_delegated_delta: 0,
            total_extra_balance: 0,
        },
    )?;

    let extra_unstake = 10;

    builder.add_claim_ticket(
        random_pubkey(&mut rng),
        ClaimTicket {
            beneficiary: random_pubkey(&mut rng),
            lamports_amount: stake_delegation - extra_unstake,
            created_epoch: 0,
        },
    )?;

    let initial_reflection = builder.build(&rent);

    let mut account_builder = AccountsBuilder::new_random(&initial_reflection, &mut rng, 0, 0);
    account_builder.random_fill(&mut rng);

    let mut test =
        IntegrationTest::start_synthetic(&account_builder, HashMap::new(), &mut rng).await?;

    println!("reflection: {:?}", test.reflection);

    let epoch_schedule = test.context.genesis_config().epoch_schedule;
    // Move to the end of epoch
    let clock = test.get_clock().await;
    test.move_to_slot(
        epoch_schedule.get_last_slot_in_epoch(clock.epoch)
            - initial_reflection.slots_for_stake_delta / 2,
    )
    .await;
    let clock = test.get_clock().await;

    assert_eq!(
        test.state
            .stake_delta(test.reflection.actual_reserve_balance),
        -((stake_delegation - extra_unstake) as i128)
    );

    let stake_split_keypair = Arc::new(Keypair::generate(&mut rng));
    test.builder.begin();
    test.builder.create_account(
        stake_split_keypair.clone(),
        std::mem::size_of::<StakeState>(),
        &stake::program::ID,
        &rent,
        "split_stake_account",
    )?;
    test.builder.deactivate_stake(
        &test.state,
        account_builder.stakes.get(0).unwrap().address,
        stake_split_keypair,
        test.fee_payer_signer(),
        0,
        0,
    );
    test.builder.commit();
    test.execute().await;

    let mut expected_reflection = initial_reflection.clone();
    let validator = expected_reflection
        .validators
        .iter_mut()
        .next()
        .expect("Validators must not be empty")
        .1;
    validator.active_balance = 0;
    validator.stake_count = 0;
    // validator.last_stake_delta_epoch is not changed because we can have multiple whole stake deactivations per epoch
    expected_reflection.total_cooling_down += stake_delegation;
    expected_reflection.cooling_down_stakes += 1;
    expected_reflection.last_stake_delta_epoch = clock.epoch;

    assert_json_eq!(test.reflection, expected_reflection);

    Ok(())
}

#[test(tokio::test)]
async fn test_deactivate_stake_split() -> anyhow::Result<()> {
    use rand_chacha::rand_core::SeedableRng;
    let mut rng = ChaChaRng::from_seed([
        55, 65, 35, 181, 232, 140, 40, 28, 51, 13, 253, 193, 103, 76, 221, 52, 240, 210, 15, 145,
        149, 197, 29, 179, 44, 5, 98, 201, 110, 56, 22, 219,
    ]);
    let rent = Rent::default(); // must be the equal to actual rent sysvar in blockchain. Will be checked later

    let mut builder = marinade_reflection::builder::Builder::default();
    builder.set_min_stake(LAMPORTS_PER_SOL);
    builder.set_cooling_down_stakes(0);
    builder.set_total_cooling_down(0);
    builder.set_available_reserve_balance(0);
    builder.set_actual_reserve_balance(rent.minimum_balance(spl_token::state::Account::LEN));
    let params = RandomBuildParams::default();
    builder.random_fill(&mut rng, &params, &rent); // basic state

    // Validator for test
    let validator_vote = random_pubkey(&mut rng);
    let stake_delegation = 20 * LAMPORTS_PER_SOL;
    builder.add_validator(
        validator_vote,
        Validator {
            active_balance: stake_delegation,
            stake_count: 1,
            score: Uniform::from(1..200).sample(&mut rng),
            last_stake_delta_epoch: Epoch::MAX,
            total_delegated_delta: 0,
            total_extra_balance: 0,
        },
    )?;

    let unstake_amount = 2 * LAMPORTS_PER_SOL;

    builder.add_claim_ticket(
        random_pubkey(&mut rng),
        ClaimTicket {
            beneficiary: random_pubkey(&mut rng),
            lamports_amount: unstake_amount,
            created_epoch: 0,
        },
    )?;

    let initial_reflection = builder.build(&rent);

    let mut account_builder = AccountsBuilder::new_random(&initial_reflection, &mut rng, 0, 0);
    account_builder.random_fill(&mut rng);

    let mut test =
        IntegrationTest::start_synthetic(&account_builder, HashMap::new(), &mut rng).await?;

    println!("reflection: {:?}", test.reflection);

    let epoch_schedule = test.context.genesis_config().epoch_schedule;
    // Move to the end of epoch
    let clock = test.get_clock().await;
    test.move_to_slot(
        epoch_schedule.get_last_slot_in_epoch(clock.epoch)
            - initial_reflection.slots_for_stake_delta / 2,
    )
    .await;
    let clock = test.get_clock().await;

    assert_eq!(
        test.state
            .stake_delta(test.reflection.actual_reserve_balance),
        -(unstake_amount as i128)
    );

    let stake_split_keypair = Arc::new(Keypair::generate(&mut rng));
    test.builder.begin();
    test.builder.create_account(
        stake_split_keypair.clone(),
        std::mem::size_of::<StakeState>(),
        &stake::program::ID,
        &rent,
        "split_stake_account",
    )?;
    test.builder.deactivate_stake(
        &test.state,
        account_builder.stakes.get(0).unwrap().address,
        stake_split_keypair,
        test.fee_payer_signer(),
        0,
        0,
    );
    test.builder.commit();
    test.execute().await;

    let mut expected_reflection = initial_reflection.clone();
    let validator = expected_reflection
        .validators
        .iter_mut()
        .next()
        .expect("Validators must not be empty")
        .1;
    validator.active_balance -= unstake_amount;
    validator.last_stake_delta_epoch = clock.epoch;
    expected_reflection.total_cooling_down += unstake_amount;
    expected_reflection.cooling_down_stakes += 1;
    expected_reflection.last_stake_delta_epoch = clock.epoch;

    assert_json_eq!(test.reflection, expected_reflection);

    Ok(())
}
