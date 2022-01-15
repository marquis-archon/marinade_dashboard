use crate::{
    change_value, initialize::InitializeInputWithSeeds, integration_test::IntegrationTest,
};
use marinade_finance_offchain_sdk::anchor_lang::solana_program::{
    native_token::LAMPORTS_PER_SOL,
    stake::{self, state::StakeState},
    system_instruction, system_program,
};
use marinade_finance_offchain_sdk::spl_associated_token_account::{
    create_associated_token_account, get_associated_token_address,
};
use marinade_finance_offchain_sdk::{
    instruction_helpers::InstructionHelpers,
    marinade_finance::{located::Located, Fee, MAX_REWARD_FEE},
};
use rand::{distributions::Uniform, prelude::Distribution, SeedableRng};
use rand_chacha::ChaChaRng;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer};
use std::{str::FromStr, sync::Arc};
use test_env_log::test;

#[test(tokio::test)]
async fn test_merge_success() -> anyhow::Result<()> {
    let mut rng = ChaChaRng::from_seed([
        51, 148, 143, 242, 253, 232, 2, 190, 185, 234, 38, 96, 87, 223, 225, 245, 128, 75, 35, 241,
        18, 127, 191, 39, 153, 194, 230, 112, 214, 90, 205, 176,
    ]);

    let input = InitializeInputWithSeeds::random(&mut rng);
    let mut test = IntegrationTest::start(&input).await?;

    let validator = Arc::new(Keypair::generate(&mut rng));
    let vote = Arc::new(Keypair::generate(&mut rng));

    test.add_validator(validator, vote.clone(), 0x100);

    let destination_stake = test
        .create_activated_stake_account(&vote.pubkey(), 10 * LAMPORTS_PER_SOL)
        .await;

    let source_stake = test
        .create_activated_stake_account(&vote.pubkey(), 2 * LAMPORTS_PER_SOL)
        .await;

    let mint_to = get_associated_token_address(&test.fee_payer(), &test.state.msol_mint);
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

    test.builder.deposit_stake_account(
        &test.state,
        destination_stake.pubkey(),
        test.fee_payer_signer(),
        mint_to,
        0,
        vote.pubkey(),
        test.fee_payer_signer(),
    );
    test.builder.deposit_stake_account(
        &test.state,
        source_stake.pubkey(),
        test.fee_payer_signer(),
        mint_to,
        0,
        vote.pubkey(),
        test.fee_payer_signer(),
    );
    test.execute().await;
    assert_eq!(test.state.stake_system.stake_count(), 2);
    test.builder.merge_stakes(
        &test.state,
        destination_stake.pubkey(),
        0,
        source_stake.pubkey(),
        1,
        0,
    );
    test.execute().await;
    assert_eq!(test.state.stake_system.stake_count(), 1);
    assert_eq!(test.get_sol_balance(&source_stake.pubkey()).await, 0);

    Ok(())
}
