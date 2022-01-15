//
// Integration Test
// add & remove validators
//
use std::sync::Arc;

use crate::{initialize::InitializeInputWithSeeds, integration_test::*};
use marinade_finance_offchain_sdk::spl_associated_token_account::get_associated_token_address;
use marinade_finance_offchain_sdk::{
    instruction_helpers::InstructionHelpers, marinade_finance::State,
};
use rand::{distributions::Uniform, prelude::Distribution, CryptoRng, RngCore, SeedableRng};
use rand_chacha::ChaChaRng;
use solana_program::native_token::{lamports_to_sol, LAMPORTS_PER_SOL};
use solana_sdk::{
    native_token::sol_to_lamports,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use test_env_log::test;

pub async fn do_add_validator(
    validator_keypair: Arc<Keypair>,
    vote_keypair: Arc<Keypair>,
    score: u32,
    test: &mut IntegrationTest,
) {
    //
    test.add_validator(validator_keypair, vote_keypair, score);
    test.execute().await;

    println!(
        "validator_list.len():{}  total_validator_score:{}  total_active_balance:{}",
        test.state.validator_system.validator_list.len(),
        test.state.validator_system.total_validator_score,
        test.state.validator_system.total_active_balance
    );

    assert_eq!(test.state.validator_system.validator_list.len(), 1);
    assert_eq!(test.state.validator_system.total_validator_score, score);
    assert_eq!(test.state.validator_system.total_active_balance, 0);
}

#[test(tokio::test)]
async fn test_add_validator() -> anyhow::Result<()> {
    let mut rng = ChaChaRng::from_seed([
        102, 221, 10, 71, 130, 126, 115, 217, 99, 44, 159, 62, 28, 73, 214, 87, 103, 93, 100, 157,
        203, 46, 9, 20, 242, 202, 225, 90, 179, 205, 107, 235,
    ]);
    let input = InitializeInputWithSeeds::random(&mut rng);
    let mut test = IntegrationTest::start(&input).await?;

    // let mut user = test
    //     .create_test_user("test_add_val_user", 200 * LAMPORTS_PER_SOL)
    //     .await;

    let validator_keypair = Arc::new(Keypair::new());
    let validator_vote_keypair = Arc::new(Keypair::new());

    let score: u32 = 100_000;

    do_add_validator(validator_keypair, validator_vote_keypair, score, &mut test).await;

    // add four more
    test.add_test_validators().await;

    assert_eq!(test.state.validator_system.validator_list.len(), 5);
    assert_eq!(
        test.state.validator_system.total_validator_score,
        300_000 + score
    );
    assert_eq!(test.state.validator_system.total_active_balance, 0);

    Ok(())
}
