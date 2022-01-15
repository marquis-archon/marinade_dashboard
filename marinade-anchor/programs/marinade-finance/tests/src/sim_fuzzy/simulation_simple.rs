#![allow(unused_variables)]
#![allow(dead_code)]
//
// Integration Test
// simple basic flow
// cast: Alice, Bob & Carol
//
use std::ops::Deref;
use std::sync::Arc;

use crate::{initialize::InitializeInputWithSeeds, integration_test::*};
use marinade_finance_offchain_sdk::anchor_lang::solana_program::native_token::{
    lamports_to_sol, LAMPORTS_PER_SOL,
};
use marinade_finance_offchain_sdk::spl_associated_token_account::get_associated_token_address;
use marinade_finance_offchain_sdk::{
    instruction_helpers::InstructionHelpers,
    marinade_finance::{calc::proportional, liq_pool::LiqPoolHelpers, ConfigMarinadeParams, State},
};
use rand::{distributions::Uniform, prelude::Distribution, CryptoRng, RngCore, SeedableRng};
use rand_chacha::ChaChaRng;
use solana_sdk::{
    native_token::sol_to_lamports,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use test_env_log::test;

use crate::integration_test::{
    delayed_unstake::*, deposit_sol_liquid_unstake::*, test_add_remove_liquidity::*,
};

#[test(tokio::test)]
async fn sim_test_simple() -> anyhow::Result<()> {
    let mut rng = ChaChaRng::from_seed([
        246, 34, 144, 55, 47, 61, 148, 182, 151, 191, 76, 252, 192, 152, 78, 54, 178, 145, 223,
        139, 45, 36, 59, 150, 119, 0, 173, 229, 255, 84, 164, 161,
    ]);

    let input = InitializeInputWithSeeds::random(&mut rng);
    let mut test = IntegrationTest::start(&input).await?;

    println!("--- initial state {:?}", &test.state.deref());

    //show state accounts balances
    //sim.show_sps_staked_balances();
    //---- alice
    let mut alice = test
        .create_test_user("alice", 500_000 * LAMPORTS_PER_SOL)
        .await;
    //---- deposit-stake SOL
    let alice_deposit_stake_amount = 10_000 * LAMPORTS_PER_SOL;
    do_deposit_sol(&mut alice, alice_deposit_stake_amount, &mut test).await;
    //---- bob
    //---- bob deposits sol too
    let mut bob = test
        .create_test_user("bob", 50_000 * LAMPORTS_PER_SOL)
        .await;
    let bob_dep_and_stake = 20_000 * LAMPORTS_PER_SOL;
    do_deposit_sol(&mut bob, bob_dep_and_stake, &mut test).await;

    //---- carol
    //---- carol adds liquidity sol too
    let mut carol = test
        .create_test_user("carol", 50_000 * LAMPORTS_PER_SOL)
        .await;
    let carol_add_liquidity_amount = 25_000 * LAMPORTS_PER_SOL;

    println!("-------------------------------");
    println!("------- carol adds liquidity --");
    do_add_liquidity(&mut carol, carol_add_liquidity_amount, &mut test)
        .await
        .unwrap();

    println!("----------------------------------");
    println!("------- small qty add-remove liq --");
    {
        // Configure marinade to have a specific deposit min limit
        let min_deposit = 673817;
        let mut config_params = ConfigMarinadeParams::default();
        config_params.min_deposit = Some(min_deposit);
        test.builder
            .config_marinade(&test.state, test.admin_authority.clone(), config_params)
            .unwrap();
        test.execute().await;
        assert_eq!(test.state.min_deposit, min_deposit);

        let too_small_qty = min_deposit - 1;
        const ERR_CODE_NUMBER_TOO_LOW: u32 = 0x2000;
        match do_add_liquidity(&mut bob, too_small_qty, &mut test).await {
            Ok(()) => debug_assert!(false, "expected err got Ok"),
            Err(ERR_CODE_NUMBER_TOO_LOW) => {
                println!("(expected tx failure 0x{:x})", ERR_CODE_NUMBER_TOO_LOW)
            }
            Err(x) => debug_assert!(false, "expected err(ERR_CODE_NUMBER_TOO_LOW) got 0x{:x}", x),
        }

        {
            let small_qty = test.state.min_deposit;
            let bob_lp_pubkey = &bob.lp_token_account_pubkey(&mut test);
            let prev_value = test.get_token_balance_or_zero(bob_lp_pubkey).await;
            do_add_liquidity(&mut bob, small_qty, &mut test)
                .await
                .unwrap();
            // TODO: remove < test.state.min_deposit in SOLs?
            do_remove_liquidity(&mut bob, small_qty, &mut test).await;
            assert_eq!(test.get_token_balance(bob_lp_pubkey).await, prev_value);
        }
    }

    /*

    // add four validators
    test.add_test_validators().await;

    //---- test distribute_staking
    //sim.show_sps_staked_balances();
    println!("----------------------------------");
    println!("------- test distribute_staking --");
    for n in 0..4 {
      println!("------- call #{} to distribute_staking", n);
      let distribute_result = call!(
        sim.operator,
        metapool.distribute_staking(),
        gas = 125 * TGAS
      );
      //check_exec_result_profile(&distribute_result);
      sim.show_sps_staked_balances();
    }
    //check the staking was distributed according to weight
    let total_staked = alice_dep_and_stake + bob_dep_and_stake;
    for n in 0..sim.sp.len() {
      let expected: u128 = total_staked * sim.weight_basis_points_vec[n] as u128 / 100;
      let staked = sim.sp_staked(n);
      assert!(
        staked >= expected - 1 && staked <= expected + 1,
        "total_for_staking:{}, sp{} balance = {}, wbp:{}, !== expected:{}",
        alice_dep_and_stake,
        n,
        &sim.sp_staked(n),
        sim.weight_basis_points_vec[n],
        expected
      );
    }

    //test unstake
    // let unstake_result = view(&sim.sp[0],"unstake_all","{}",0,50*TGAS);
    // check_exec_result_promise(&unstake_result);
    // sim.show_sps_staked_balances();

    //-----------
    sim.show_account_info(&alice.account_id());

    println!("-------------------------");
    println!("------- alice unstakes --");
    let alice_unstaking = sol_to_lamports(6_000);
    {
      let ads_res = call!(
        alice,
        metapool.unstake(alice_unstaking.into()),
        gas = 50 * TGAS
      );
      check_exec_result(&ads_res);

      sim.show_account_info(&alice.account_id());
    }

    //------------------------------
    //---- test distribute_unstaking
    println!("------------------------------------");
    println!("------- test distribute_unstaking --");
    for n in 0..20 {
      println!("------- call #{} to distribute_unstaking", n);
      let distribute_result = call!(
        sim.operator,
        metapool.distribute_unstaking(),
        gas = 125 * TGAS
      );
      check_exec_result(&distribute_result);
      sim.show_sps_staked_balances();
      if &distribute_result.unwrap_json_value() == false {
        break;
      };
    }

    //---------------------------------
    //---- test retrieve unstaked funds
    //---------------------------------
    println!("---------------------------------------------");
    println!("------- test retrieve funds from the pools --");
    for n in 0..30 {
      println!(
        "epoch {}",
        view(&sim.get_epoch_acc, "get_epoch_height", "{}")
      );

      println!(
        "------- call #{} to get_staking_pool_requiring_retrieve()",
        n
      );
      let retrieve_result = view!(metapool.get_staking_pool_requiring_retrieve());
      let inx = retrieve_result.unwrap_json_value().as_i64().unwrap();
      println!("------- result {}", inx);

      if inx >= 0 {
        println!("------- pool #{} requires retrieve", inx);
        println!("------- pool #{} sync unstaked", inx);
        let retrieve_result_sync = call!(
          sim.operator,
          metapool.sync_unstaked_balance(inx as u16),
          gas = 200 * TGAS
        );
        check_exec_result(&retrieve_result_sync);
        println!("{:?}",&sim.sp[inx as usize].account().unwrap());
        println!("------- pool #{} retrieve unstaked", inx);
        let retrieve_result_2 = call!(
          sim.operator,
          metapool.retrieve_funds_from_a_pool(inx as u16),
          gas = 200 * TGAS
        );
        check_exec_result(&retrieve_result_2);
      } else if inx == -3 {
        //no more funds unstaked
        break;
      }

      for epoch in 1..4 {
        //make a dummy txn to advance the epoch
        call(
          &sim.owner,
          &sim.get_epoch_acc,
          "set_i32",
          &format!(r#"{{"num":{}}}"#, inx).to_string(),
          0,
          10 * TGAS,
        );
        println!(
          "epoch {}",
          view(&sim.get_epoch_acc, "get_epoch_height", "{}")
        );
      }
    }

    println!("----------------------------------------");
    println!("------- alice calls withdraw_unstaked --");
    {
      let previous = balance(&alice);
      let ads_res = call!(alice, metapool.withdraw_unstaked(), gas = 50 * TGAS);
      check_exec_result(&ads_res);
      assert_less_than_one_milli_near_diff_balance(
        "withdraw_unstaked",
        balance(&alice),
        previous + alice_unstaking,
      );
    }

    println!("---------------------------");
    println!("------- bob liquid-unstakes");
    {
      sim.show_account_info(&bob.account_id());
      sim.show_account_info(&carol.account_id());
      sim.show_account_info(NSLP_INTERNAL_ACCOUNT);
      let vr1 = view!(metapool.get_contract_state());
      print_vec_u8("contract_state", &vr1.unwrap());
      let vr2 = view!(metapool.get_contract_params());
      print_vec_u8("contract_params", &vr2.unwrap());

      let previous = balance(&bob);
      const TO_SELL: u128 = 20_000 * NEAR;
      const MIN_REQUESTED: u128 = 19_300 * NEAR; //7% discount
      let dbp = view!(metapool.nslp_get_discount_basis_points(TO_SELL.into()));
      print_vec_u8("metapool.nslp_get_discount_basis_points", &dbp.unwrap());

      let lu_res = call!(
        bob,
        metapool.liquid_unstake(U128::from(sol_to_lamports(20_000)), U128::from(MIN_REQUESTED)),
        0,
        100 * TGAS
      );
      check_exec_result(&lu_res);
      println!("liquid unstake result {}", &lu_res.unwrap_json_value());

      let bob_info = sim.show_account_info(&bob.account_id());
      let carol_info = sim.show_account_info(&carol.account_id());
      let nslp_info = sim.show_account_info(NSLP_INTERNAL_ACCOUNT);

      assert_eq!(as_u128(&bob_info["meta"]), 250 * NEAR);
      assert_eq!(as_u128(&carol_info["meta"]), 1750 * NEAR);
    }

    println!("-----------------------------------");
    println!("------- carol will remove liquidity");
    {
      const AMOUNT: u128 = 100_000 * NEAR;
      println!("-- pre ");
      let pre_balance = balance(&carol);
      println!("pre balance {}", yton(pre_balance));
      let carol_info_pre = sim.show_account_info(&carol.account_id());
      println!("-- nslp_remove_liquidity");
      let res = call!(
        carol,
        metapool.nslp_remove_liquidity(U128::from(AMOUNT)),
        gas = 100 * TGAS
      );
      check_exec_result(&res);
      //let res_json = serde_json::from_str(std::str::from_utf8(&res.unwrap()).unwrap()).unwrap();
      let res_json = res.unwrap_json_value();
      println!("-- result: {:?}", res_json);
      println!("-- after ");
      let carol_info = sim.show_account_info(&carol.account_id());
      let new_balance = balance(&carol);
      println!("new balance {}", yton(new_balance));
      let stnear = as_u128(&carol_info["stnear"]);
      println!("stnear {}", yton(stnear));
      assert_less_than_one_milli_near_diff_balance(
        "rem.liq",
        new_balance + stnear - pre_balance,
        AMOUNT,
      );
    }
    */
    Ok(())
}
