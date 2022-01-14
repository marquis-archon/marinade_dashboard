use crate::{
    change_value, initialize::InitializeInputWithSeeds, integration_test::IntegrationTest,
};
use marinade_finance_offchain_sdk::{
    instruction_helpers::InstructionHelpers,
    marinade_finance::{located::Located, ConfigMarinadeParams, Fee, MAX_REWARD_FEE},
};

use marinade_finance_offchain_sdk::anchor_lang::solana_program::native_token::{
    sol_to_lamports, LAMPORTS_PER_SOL,
};
use rand::{distributions::Uniform, prelude::Distribution, SeedableRng};
use rand_chacha::ChaChaRng;
use solana_sdk::signature::Signer;

use std::str::FromStr;
use test_env_log::test;

#[test(tokio::test)]
async fn test_set_lp_params_and_config() -> anyhow::Result<()> {
    let mut rng = ChaChaRng::from_seed([
        170, 133, 130, 141, 229, 166, 42, 178, 43, 7, 129, 69, 116, 163, 27, 192, 208, 125, 122,
        17, 144, 182, 65, 5, 212, 238, 200, 201, 142, 177, 179, 93,
    ]);

    let mut test = IntegrationTest::start(&InitializeInputWithSeeds::random(&mut rng)).await?;
    let min_fee_before = test.state.as_ref().liq_pool.lp_min_fee;
    let max_fee_before = test.state.as_ref().liq_pool.lp_max_fee;
    let liquidity_target_before = test.state.as_ref().liq_pool.lp_liquidity_target;
    let min_fee_new = change_value(min_fee_before, || {
        Fee::from_basis_points(Uniform::from(0..MAX_REWARD_FEE).sample(&mut rng))
    });
    let max_fee_new = change_value(max_fee_before, || {
        Fee::from_basis_points(
            Uniform::from(min_fee_new.basis_points..=MAX_REWARD_FEE).sample(&mut rng),
        )
    });
    let liquidity_target_new = change_value(liquidity_target_before, || {
        Uniform::from((50u64 * LAMPORTS_PER_SOL)..=u64::MAX).sample(&mut rng)
    });

    // Confirm that setting each value will result in a state change.
    // assert_ne!(min_fee_before, min_fee_new);
    // assert_ne!(max_fee_before, max_fee_new);
    // assert_ne!(liquidity_target_before, liquidity_target_new);

    // Execute SetLpParams by applying helper::set_lp_params to set_lp_params_new.
    test.builder
        .set_lp_params(
            &test.state,
            test.admin_authority.clone(),
            min_fee_new,
            max_fee_new,
            liquidity_target_new,
        )
        .unwrap();
    test.execute().await;
    let min_fee_after = test.state.as_ref().liq_pool.lp_min_fee;
    let max_fee_after = test.state.as_ref().liq_pool.lp_max_fee;
    let liquidity_target_after = test.state.as_ref().liq_pool.lp_liquidity_target;
    assert_eq!(min_fee_after, min_fee_new);
    assert_eq!(max_fee_after, max_fee_new);
    assert_eq!(liquidity_target_after, liquidity_target_new);

    // marinade-config

    // set fee & min_stake
    let new_fee = Fee::from_str("0.75").unwrap();
    let min_stake = sol_to_lamports(0.553);
    let slots_for_stake_delta = 9_000;
    let min_deposit = sol_to_lamports(0.012);
    let min_withdraw = sol_to_lamports(0.002);
    let staking_sol_cap = sol_to_lamports(1000.22);
    let liquidity_sol_cap = sol_to_lamports(20232.1);
    test.builder
        .config_marinade(
            &test.state,
            test.admin_authority.clone(),
            ConfigMarinadeParams {
                rewards_fee: Some(new_fee),
                slots_for_stake_delta: Some(slots_for_stake_delta),
                min_stake: Some(min_stake),
                min_deposit: Some(min_deposit),
                min_withdraw: Some(min_withdraw),
                staking_sol_cap: Some(staking_sol_cap),
                liquidity_sol_cap: Some(liquidity_sol_cap),
                auto_add_validator_enabled: None,
            },
        )
        .unwrap();

    test.execute().await;
    assert_eq!(test.state.as_ref().reward_fee, new_fee);
    assert_eq!(
        test.state.as_ref().stake_system.slots_for_stake_delta,
        slots_for_stake_delta
    );
    assert_eq!(test.state.as_ref().stake_system.min_stake, min_stake);
    assert_eq!(test.state.as_ref().min_deposit, min_deposit);
    assert_eq!(test.state.as_ref().min_withdraw, min_withdraw);
    assert_eq!(test.state.as_ref().staking_sol_cap, staking_sol_cap);
    assert_eq!(
        test.state.as_ref().liq_pool.liquidity_sol_cap,
        liquidity_sol_cap
    );

    // Test max fee
    // set fee too high
    test.builder
        .config_marinade(
            &test.state,
            test.admin_authority.clone(),
            ConfigMarinadeParams {
                rewards_fee: Some(Fee::from_str("11").unwrap()),
                min_stake: None,
                slots_for_stake_delta: None,
                min_deposit: None,
                min_withdraw: None,
                staking_sol_cap: None,
                liquidity_sol_cap: None,
                auto_add_validator_enabled: None,
            },
        )
        .unwrap();
    // should fail with FEE-TOO-HIGH
    const ERR_FEE_TOO_HIGH: u32 = 0x1100;
    match test.try_execute().await {
        Ok(()) => debug_assert!(false, "expected err got Ok"),
        Err(ERR_FEE_TOO_HIGH) => {
            println!("(expected tx failure 0x{:x})", ERR_FEE_TOO_HIGH)
        }
        Err(x) => debug_assert!(false, "expected ERR_FEE_TOO_HIGH got 0x{:x}", x),
    }

    // Test min_stake
    // set too low
    let min_stake = test.state.rent_exempt_for_token_acc;
    test.builder
        .config_marinade(
            &test.state,
            test.admin_authority.clone(),
            ConfigMarinadeParams {
                min_stake: Some(min_stake),
                rewards_fee: None,
                slots_for_stake_delta: None,
                min_deposit: None,
                min_withdraw: None,
                staking_sol_cap: None,
                liquidity_sol_cap: None,
                auto_add_validator_enabled: None,
            },
        )
        .unwrap();
    // should fail with NUMBER_TOO_LOW
    const NUMBER_TOO_LOW: u32 = 0x2000;
    match test.try_execute().await {
        Ok(()) => debug_assert!(false, "expected err got Ok"),
        Err(NUMBER_TOO_LOW) => {
            println!("(expected tx failure 0x{:x})", NUMBER_TOO_LOW)
        }
        Err(x) => debug_assert!(false, "expected NUMBER_TOO_LOW got 0x{:x}", x),
    }

    Ok(())
}

#[test(tokio::test)]
async fn test_config_validator_system() -> anyhow::Result<()> {
    // config validators

    let mut rng = ChaChaRng::from_seed([
        170, 133, 130, 141, 229, 166, 42, 178, 43, 7, 129, 69, 116, 163, 27, 192, 208, 125, 122,
        17, 144, 182, 65, 5, 212, 238, 200, 201, 142, 177, 179, 93,
    ]);
    let mut test = IntegrationTest::start(&InitializeInputWithSeeds::random(&mut rng)).await?;

    // Execute ConfigValidatorSystem
    test.builder
        .config_validator_system(&test.state, test.validator_manager_authority.clone(), 10)
        .unwrap();
    test.execute().await;
    assert_eq!(test.state.as_ref().stake_system.extra_stake_delta_runs, 10);

    Ok(())
}
