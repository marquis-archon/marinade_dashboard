use crate::{
    change_value, initialize::InitializeInputWithSeeds, integration_test::IntegrationTest,
};
use marinade_finance_offchain_sdk::{instruction_helpers::InstructionHelpers, marinade_finance::{self, ConfigMarinadeParams, Fee, MAX_REWARD_FEE, located::Located, ticket_account::TicketAccountData}};

use marinade_finance_offchain_sdk::anchor_lang::solana_program::{
    native_token::LAMPORTS_PER_SOL,
    stake::{self, state::StakeState},
    system_instruction, system_program,
};
use marinade_finance_offchain_sdk::spl_associated_token_account::{
    create_associated_token_account, get_associated_token_address,
};
use rand::{distributions::Uniform, prelude::Distribution, SeedableRng};
use rand_chacha::ChaChaRng;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};
use std::{str::FromStr, sync::Arc};
use test_env_log::test;

#[test(tokio::test)]
async fn test_unstake_unlisted() -> anyhow::Result<()> {
    let mut rng = ChaChaRng::from_seed([
        51, 148, 143, 242, 253, 232, 2, 190, 185, 234, 38, 96, 87, 223, 225, 245, 128, 75, 35, 241,
        18, 127, 191, 39, 153, 194, 230, 112, 214, 90, 205, 176,
    ]);

    let input = InitializeInputWithSeeds::random(&mut rng);
    let mut test = IntegrationTest::start(&input).await?;

    // add 4 test validators
    test.add_test_validators().await;

    // enable auto_add_validator mode
    let mut config_params = ConfigMarinadeParams::default();
    config_params.auto_add_validator_enabled = Some(true);
    test.builder
        .config_marinade(&test.state, test.admin_authority.clone(), config_params)
        .unwrap();
    test.execute().await;

    // Create another validator unlisted (should auto-add with score 0)
    let other_validator_keypair = Arc::new(Keypair::new());
    let vote = Arc::new(Keypair::new());
    test.install_validator(other_validator_keypair, vote.clone());

    // let mut alice = test.create_test_user("alice", 500_000 * LAMPORTS_PER_SOL).await;
    // let msol_acc = alice.get_or_create_msol_account(test).await;

    // create active account for the non-listed validator
    let unlisted_stake = test
        .create_activated_stake_account(&vote.pubkey(), 20 * LAMPORTS_PER_SOL)
        .await;

    let msol_account = get_associated_token_address(&test.fee_payer(), &test.state.msol_mint);
    test.builder
        .add_instruction(
            create_associated_token_account(
                &test.fee_payer(),
                &test.fee_payer(),
                &test.state.msol_mint,
            ),
            format!("create user mSOL token account"),
        )
        .unwrap();

    assert_eq!(test.state.validator_system.validator_count(), 4);

    test.builder.deposit_stake_account(
        &test.state,
        unlisted_stake.pubkey(),
        test.fee_payer_signer(),
        msol_account,
        test.state.validator_system.validator_count(), // new validator
        vote.pubkey(),
        test.fee_payer_signer(),
    );
    test.execute().await;
    assert_eq!(test.state.validator_system.validator_count(), 5);
    assert_eq!(test.state.stake_system.stake_count(), 1);

    // let mut params = crate::integration_test::delayed_unstake::DelayedUnstakeParams::new(&test.state);
    // crate::integration_test::delayed_unstake::do_order_unstake(&mut params, &mut test).await;

    // Create a empty ticket account (transfer rent-exempt lamports)
    const TICKET_ACCOUNT_SPACE: usize = 8 + std::mem::size_of::<TicketAccountData>();
    //let ticket_account_rent_exempt_lamports = test.rent.minimum_balance(TICKET_ACCOUNT_SPACE);
    let ticket_account = Arc::new(Keypair::new());
    test.builder
        .create_account(
            ticket_account.clone(),
            TICKET_ACCOUNT_SPACE,
            &marinade_finance::ID,
            &test.rent,
            "ticket-account",
        )
        .unwrap();

    let prev_cooling_down = test.state.stake_system.delayed_unstake_cooling_down;
    const ORDER_UNSTAKE_AMOUNT: u64 = 4 * LAMPORTS_PER_SOL;
    // Create a OrderUnstake instruction.
    test.builder.order_unstake(
        &test.state,
        msol_account,
        test.fee_payer_signer(), //user_msol owner & signer
        ORDER_UNSTAKE_AMOUNT,
        ticket_account.pubkey(),
        // params.user_sol.pubkey(), //ticket beneficiary
    );
    test.execute().await;

    // check if we can unstake from the new account
    let split_stake_keypair = Arc::new(Keypair::new());
    test.builder.create_account(
        split_stake_keypair.clone(),
        std::mem::size_of::<StakeState>(),
        &stake::program::ID,
        &test.rent,
        "split_stake_account",
    )?;
    test.builder.deactivate_stake(
        &test.state,
        unlisted_stake.pubkey(),
        split_stake_keypair,
        test.fee_payer_signer(),
        0,                                                 // index=0, only stake account
        test.state.validator_system.validator_count() - 1, // last one
    );
    test.execute().await;

    assert!(
        test.state.stake_system.delayed_unstake_cooling_down
            == prev_cooling_down + ORDER_UNSTAKE_AMOUNT
    );
    Ok(())
}
