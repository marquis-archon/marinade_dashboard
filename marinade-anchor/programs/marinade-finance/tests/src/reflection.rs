use crate::integration_test::IntegrationTest;
use marinade_finance_offchain_sdk::solana_sdk::{
    clock::Clock, pubkey::Pubkey, stake::state::StakeState, sysvar::rent::Rent,
};
use marinade_finance_offchain_sdk::{
    anchor_lang::solana_program::{
        clock::Epoch,
        native_token::LAMPORTS_PER_SOL,
        program_pack::Pack,
        stake::state::{Authorized, Delegation, Lockup, Meta, Stake},
    },
    marinade_finance::Fee,
};
use marinade_reflection::{
    accounts_builder::AccountsBuilder,
    builder::RandomBuildParams,
    marinade::{ClaimTicket, Validator},
    random_pubkey,
};
use rand::distributions::Distribution;
use rand::distributions::Uniform;
use rand_chacha::ChaChaRng;
use solana_vote_program::vote_state::{VoteInit, VoteState};
use std::collections::{HashMap, HashSet};
use test_env_log::test;

#[test(tokio::test)]
async fn test_reflection_write_read() -> anyhow::Result<()> {
    use rand_chacha::rand_core::SeedableRng;
    let mut rng = ChaChaRng::from_seed([
        130, 213, 252, 141, 128, 94, 190, 154, 30, 115, 169, 146, 228, 224, 66, 90, 32, 91, 210,
        225, 223, 163, 233, 213, 55, 126, 6, 252, 2, 242, 133, 142,
    ]);
    let rent = Rent::default(); // must be the equal to actual rent sysvar in blockchain. Will be checked later

    let mut builder = marinade_reflection::builder::Builder::default();
    //let rent_exempt_for_token_acc = rent.minimum_balance(spl_token::state::Account::LEN);
    builder.set_msol_mint(random_pubkey(&mut rng));
    builder.set_admin_authority(random_pubkey(&mut rng));
    builder.set_operational_sol_account(random_pubkey(&mut rng));
    builder.set_treasury_msol_account(random_pubkey(&mut rng));
    builder.set_min_stake(LAMPORTS_PER_SOL);
    builder.set_reward_fee(Fee::from_basis_points(
        Uniform::from(0..1000).sample(&mut rng),
    ));
    builder.set_validator_manager_authority(random_pubkey(&mut rng));
    builder.set_free_validator_slots(Uniform::from(100..1000).sample(&mut rng));
    builder.set_free_stake_slots(Uniform::from(100..1000).sample(&mut rng));
    let cooling_down_stake_sizes: Vec<u64> = (0..3)
        .map(|_| Uniform::from(1 * LAMPORTS_PER_SOL..5 * LAMPORTS_PER_SOL).sample(&mut rng))
        .collect();
    builder.set_total_cooling_down(cooling_down_stake_sizes.iter().sum());
    builder.set_cooling_down_stakes(cooling_down_stake_sizes.len() as u32);

    // Validators
    let validator_vote1 = random_pubkey(&mut rng);
    println!("Validator1 {}", validator_vote1);
    builder.add_validator(
        validator_vote1,
        Validator {
            active_balance: 0,
            stake_count: 0,
            score: Uniform::from(10..100).sample(&mut rng),
            last_stake_delta_epoch: Epoch::MAX,
            total_delegated_delta: 0, // no stakes
            total_extra_balance: 0,   // no stakes
        },
    )?;

    struct StakeData {
        delegated: u64,
        delta: u64,
        extra_balance: u64,
    }

    let validator_vote2 = random_pubkey(&mut rng);
    println!("Validator2 {}", validator_vote2);
    let validator2_stake = StakeData {
        delegated: Uniform::from(10 * LAMPORTS_PER_SOL..100 * LAMPORTS_PER_SOL).sample(&mut rng),
        delta: Uniform::from(LAMPORTS_PER_SOL..10 * LAMPORTS_PER_SOL).sample(&mut rng),
        extra_balance: Uniform::from(LAMPORTS_PER_SOL..10 * LAMPORTS_PER_SOL).sample(&mut rng),
    };
    builder.add_validator(
        validator_vote2,
        Validator {
            active_balance: validator2_stake.delegated,
            stake_count: 1,
            score: Uniform::from(10..100).sample(&mut rng),
            last_stake_delta_epoch: Epoch::MAX,
            total_delegated_delta: validator2_stake.delta,
            total_extra_balance: validator2_stake.extra_balance,
        },
    )?;

    let validator3_stakes: Vec<StakeData> = (0..10)
        .map(|_| StakeData {
            delegated: Uniform::from(10 * LAMPORTS_PER_SOL..100 * LAMPORTS_PER_SOL)
                .sample(&mut rng),
            delta: Uniform::from(LAMPORTS_PER_SOL..10 * LAMPORTS_PER_SOL).sample(&mut rng),
            extra_balance: Uniform::from(LAMPORTS_PER_SOL..10 * LAMPORTS_PER_SOL).sample(&mut rng),
        })
        .collect();
    let validator_vote3 = random_pubkey(&mut rng);
    println!("Validator3 {}", validator_vote3);
    builder.add_validator(
        validator_vote3,
        Validator {
            active_balance: validator3_stakes.iter().map(|data| data.delegated).sum(),
            stake_count: validator3_stakes.len() as u32,
            score: Uniform::from(10..100).sample(&mut rng),
            last_stake_delta_epoch: Epoch::MAX,
            total_delegated_delta: validator3_stakes.iter().map(|data| data.delta).sum(),
            total_extra_balance: validator3_stakes
                .iter()
                .map(|data| data.extra_balance)
                .sum(),
        },
    )?;

    builder.set_lp_mint(random_pubkey(&mut rng));
    builder.set_lp_supply(Uniform::from(0..1000).sample(&mut rng));
    builder.set_actual_liq_pool_sol_amount(Uniform::from(0..1000).sample(&mut rng));
    builder.set_actual_liq_pool_msol_amount(Uniform::from(0..1000).sample(&mut rng));
    builder.set_lp_liquidity_target(Uniform::from(0..1000).sample(&mut rng) * LAMPORTS_PER_SOL);
    builder.set_lp_max_fee(Fee::from_basis_points(300));
    builder.set_lp_min_fee(Fee::from_basis_points(30));
    builder.set_lp_treasury_cut(Fee::from_basis_points(2500));
    builder.set_available_reserve_balance(
        Uniform::from(10 * LAMPORTS_PER_SOL..1000 * LAMPORTS_PER_SOL).sample(&mut rng),
    );
    builder.set_msol_supply(Uniform::from(0..1000).sample(&mut rng));
    builder.set_slots_for_stake_delta(Uniform::from(0..1000).sample(&mut rng));
    builder.set_last_stake_delta_epoch(Epoch::MAX);
    builder.set_min_deposit(2);
    builder.set_min_withdraw(3);

    let claim_ticket_keys: Vec<Pubkey> = (0..5).map(|_| random_pubkey(&mut rng)).collect();
    for key in &claim_ticket_keys {
        builder.add_claim_ticket(
            *key,
            ClaimTicket {
                beneficiary: random_pubkey(&mut rng),
                lamports_amount: Uniform::from(1..LAMPORTS_PER_SOL).sample(&mut rng),
                created_epoch: 0, // TODO
            },
        )?;
    }

    let initial_reflection = builder.build(&rent);

    let mut account_builder = AccountsBuilder::new_random(&initial_reflection, &mut rng, 0, 0);
    let clock = Clock::default(); // TODO?
                                  // Install validators
    for vote_address in initial_reflection.validators.keys() {
        let validator_identity = random_pubkey(&mut rng);
        account_builder.add_validator(
            *vote_address,
            VoteState::new(
                &VoteInit {
                    node_pubkey: validator_identity,
                    authorized_voter: validator_identity,
                    ..VoteInit::default()
                },
                &clock,
            ),
        )?;
    }
    // Install stakes

    // For validator 2
    account_builder.add_stake(marinade_reflection::accounts_builder::StakeBuilder {
        address: random_pubkey(&mut rng),
        voter_pubkey: validator_vote2,
        is_active: true,
        stake: validator2_stake.delegated + validator2_stake.delta,
        last_update_delegated_lamports: validator2_stake.delegated,
        last_update_epoch: 0,
        extra_balance: validator2_stake.extra_balance,
    })?;
    // for Validator 3
    for stake_data in validator3_stakes {
        account_builder.add_stake(marinade_reflection::accounts_builder::StakeBuilder {
            address: random_pubkey(&mut rng),
            voter_pubkey: validator_vote3,
            stake: stake_data.delegated + stake_data.delta,
            is_active: true,
            last_update_delegated_lamports: stake_data.delegated,
            last_update_epoch: 0,
            extra_balance: stake_data.extra_balance,
        })?;
    }
    // Cooling down
    for stake_size in cooling_down_stake_sizes {
        account_builder.add_stake(marinade_reflection::accounts_builder::StakeBuilder {
            address: random_pubkey(&mut rng),
            voter_pubkey: random_pubkey(&mut rng),
            stake: stake_size,
            is_active: false,
            last_update_delegated_lamports: stake_size,
            last_update_epoch: 0,
            extra_balance: Uniform::from(1 * LAMPORTS_PER_SOL..5 * LAMPORTS_PER_SOL)
                .sample(&mut rng),
        })?;
    }

    account_builder.shuffle_stakes(&mut rng);

    // All check inside IntegrationTest
    let _ = IntegrationTest::start_synthetic(&account_builder, HashMap::new(), &mut rng).await?;

    Ok(())
}

#[test(tokio::test)]
async fn test_reflection_random_builder_write_read() -> anyhow::Result<()> {
    use rand_chacha::rand_core::SeedableRng;
    let mut rng = ChaChaRng::from_seed([
        195, 238, 184, 125, 114, 86, 121, 17, 104, 29, 167, 51, 204, 176, 134, 206, 127, 85, 224,
        131, 74, 138, 72, 233, 92, 95, 105, 6, 8, 112, 35, 177,
    ]);
    let rent = Rent::default(); // must be the equal to actual rent sysvar in blockchain. Will be checked later

    let mut builder = marinade_reflection::builder::Builder::default();
    let params = RandomBuildParams::pick(&mut builder, &mut rng);
    builder.random_fill(&mut rng, &params, &rent);
    builder.fill_random_claim_tickets(
        100..builder.total_lamports_under_control(),
        1..100,
        1,
        &mut rng,
    )?;
    let initial_claim_tickets: HashSet<Pubkey> = builder.claim_ticket_keys();
    let initial_reflection = builder.build(&rent);
    assert_eq!(
        initial_claim_tickets,
        initial_reflection.claim_ticket_keys()
    );

    let mut account_builder = AccountsBuilder::new_random(&initial_reflection, &mut rng, 0, 0);
    account_builder.random_fill(&mut rng);

    // All check inside IntegrationTest
    let _ = IntegrationTest::start_synthetic(&account_builder, HashMap::new(), &mut rng).await?;

    Ok(())
}
