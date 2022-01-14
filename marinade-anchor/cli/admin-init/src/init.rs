use crate::{Command, Common};
use anyhow::{anyhow, bail};
use cli_common::marinade_finance::{liq_pool::LiqPool, state::State, Fee};
use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::solana_sdk::{
    program_pack::Pack, rent::Rent, signature::Keypair, signature::Signer, system_program,
    sysvar::rent,
};
use cli_common::spl_token;
use cli_common::spl_token::state::Mint;
use cli_common::{
    instruction_helpers::InstructionHelpers, rpc_client_helpers::RpcClientHelpers,
    transaction_builder::TransactionBuilder, ExpandedPath, InputKeypair, InputPubkey,
};
use log::{error, info, warn};
use std::{fs::File, io::Write, sync::Arc};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct Init {
    #[structopt(short = "c", default_value = "keys/creator.json")]
    creator_authority: InputKeypair,
    #[structopt(short = "i")]
    instance: Option<InputKeypair>, // Generate random if None
    #[structopt(long = "output-instance")]
    output_instance: Option<ExpandedPath>, // write instance pubkey here. Useful for random generated instance

    #[structopt(short = "t")]
    treasury_msol_authority: Option<InputPubkey>, // fee payer by default

    // #[structopt(default_value = "~/.config/mardmin/treasury_msol_account.json")]
    // treasury_msol_account: InputPubkey,
    #[structopt(short = "m")]
    msol_mint: Option<InputPubkey>, // Generate random if None

    #[structopt(long)]
    msol_mint_owner: Option<InputKeypair>, // Used if msol exists (fee payer by default)

    #[structopt(short = "a")]
    admin_authority: Option<InputPubkey>, // fee payer by default, use multisig auth on mainnet

    #[structopt(short = "o")]
    operational_sol_account: Option<InputPubkey>, // fee payer by default

    #[structopt(short = "d")]
    validator_manager_authority: Option<InputPubkey>, // admin by default

    #[structopt(long, default_value = "10000")]
    max_stake_accounts: u32,

    #[structopt(long, default_value = "1000")]
    max_validators: u32,

    #[structopt(long, default_value = "2")]
    fee: Fee,

    #[structopt(long, short = "l")]
    lp_mint: Option<InputPubkey>, // Generate random if None

    #[structopt(long)]
    lp_mint_owner: Option<InputKeypair>, // Used if lp exists (fee payer by default)

    #[structopt(short = "w", default_value = "18000")]
    slots_for_stake_delta: u64,
}

impl Command for Init {
    fn process(self, common: Common, client: Arc<RpcClient>) -> anyhow::Result<()> {
        info!("Initialize instance {:?}", self.instance);
        let builder = TransactionBuilder::limited(common.fee_payer.as_keypair());

        let rent: Rent = bincode::deserialize(&client.get_account_data(&rent::id())?)?;

        //check if the program is deployed
        /*let program_pub_key = marinade_finance::id();
        println!("Program pubkey {}", program_pub_key);
        let _program = client
            .get_account(&program_pub_key)
            .expect("program not deployed!");*/
        info!("Marinade program {}", cli_common::marinade_finance::ID);

        let instance = if let Some(instance) = self.instance {
            if client
                .get_account_retrying(&instance.as_pubkey())?
                .is_some()
            {
                error!(
                    "Marinade state account {} already exists",
                    instance.as_pubkey()
                );
                bail!(
                    "Marinade state account {} already exists",
                    instance.as_pubkey()
                );
            }
            instance.as_keypair()
        } else {
            let instance = Arc::new(Keypair::new());
            warn!(
                "Generating random marinade state account {}",
                instance.pubkey()
            );
            instance
        };

        if let Some(output_instance) = self.output_instance {
            File::create(output_instance.as_path())?
                .write_all(instance.pubkey().to_string().as_bytes())?; // bs58 representation
        }

        let mut builder = builder.initialize(instance, self.creator_authority.as_keypair())?;
        // msol_mint
        println!("msol_mint {:?}", self.msol_mint);
        let msol_mint = if let Some(msol_mint) = self.msol_mint {
            if let Some(account) = client.get_account_retrying(&msol_mint.as_pubkey())? {
                if account.owner != spl_token::ID {
                    error!(
                        "Wrong mSOL mint account {} owner {}",
                        &msol_mint.as_pubkey(),
                        account.owner
                    );
                    bail!(
                        "Wrong mSOL mint account {} owner {}",
                        &msol_mint.as_pubkey(),
                        account.owner
                    );
                }

                let mint = Mint::unpack_from_slice(&account.data).map_err(|_| {
                    error!(
                        "Can not parse account {} as SPL token mint",
                        &msol_mint.as_pubkey()
                    );
                    anyhow!(
                        "Can not parse account {} as SPL token mint",
                        &msol_mint.as_pubkey()
                    )
                })?;

                let msol_mint_owner = self.msol_mint_owner.map_or_else(
                    || common.fee_payer.as_keypair(),
                    |msol_mint_owner| msol_mint_owner.as_keypair(),
                );
                info!("Use mSOL mint {}", msol_mint.as_pubkey());
                builder.use_msol_mint(msol_mint.as_pubkey(), &mint, Some(msol_mint_owner))?;
            } else {
                info!("Create mSOL mint {}", msol_mint.as_pubkey());
                builder.create_msol_mint(
                    msol_mint.try_as_keypair().ok_or_else(|| {
                        error!(
                        "mSOL mint account {} does not exists. Please provide keypair to create it",
                        msol_mint
                    );
                        anyhow!(
                        "mSOL mint account {} does not exists. Please provide keypair to create it",
                        msol_mint
                    )
                    })?,
                    &rent,
                );
            }
            msol_mint.as_pubkey()
        } else {
            // Generate random mSOL mint
            let msol_mint = Arc::new(Keypair::new());
            warn!("Generate random mSOL mint {}", msol_mint.pubkey());
            builder.create_msol_mint(msol_mint.clone(), &rent);
            msol_mint.pubkey()
        };

        // admin authority
        let admin_authority = if let Some(admin_authority) = &self.admin_authority {
            info!("Using admin authority {}", admin_authority);
            admin_authority.as_pubkey()
        } else {
            info!("Using fee payer as admin authority");
            common.fee_payer.as_pubkey()
        };
        builder.set_admin_authority(admin_authority);

        let operational_sol_account =
            if let Some(operational_sol_account) = &self.operational_sol_account {
                info!("Use operational_sol_account = {}", operational_sol_account);
                operational_sol_account.as_pubkey()
            } else {
                info!("Use fee payer as operational_sol_account");
                common.fee_payer.as_pubkey()
            };
        if let Some(account) = client.get_account_retrying(&operational_sol_account)? {
            if account.owner != system_program::ID {
                error!(
                    "Wrong operational_sol_account {} owner {}. Must be a system account",
                    operational_sol_account, account.owner
                );
                bail!(
                    "Wrong operational_sol_account {} owner {}. Must be a system account",
                    operational_sol_account,
                    account.owner
                );
            }
        }
        builder.set_operational_sol_account(operational_sol_account);

        // Validator manager authority
        builder.use_validator_manager_authority(
            if let Some(validator_manager_authority) = &self.validator_manager_authority {
                info!(
                    "Using validator manager authority {}",
                    validator_manager_authority
                );
                validator_manager_authority.as_pubkey()
            } else {
                info!("Using admin as validator manager authority");
                admin_authority
            },
        );
        {
            // Stake list
            let stake_list_address = builder.default_stake_list_account();
            if let Some(stake_list) = client.get_account_retrying(&stake_list_address)? {
                if !cli_common::marinade_finance::check_id(&stake_list.owner) {
                    error!(
                        "Wrong stake list {} owner {}",
                        stake_list_address, stake_list.owner
                    );
                    bail!(
                        "Wrong stake list {} owner {}",
                        stake_list_address,
                        stake_list.owner
                    );
                }

                if stake_list.data.len() < 8 {
                    error!("Stake list {} is < 8 bytes", stake_list_address);
                    bail!("Stake list {} is < 8 bytes", stake_list_address);
                }

                if stake_list.data[0..8] != [0; 8] {
                    error!("Stake list {} is not empty account", stake_list_address);
                    bail!("Stake list {} is not empty account", stake_list_address);
                }
                builder.use_stake_list(stake_list_address)
            } else {
                builder.create_stake_list_with_seed(self.max_stake_accounts, &rent);
            }
        }

        // Validator list
        {
            let validator_list_address = State::default_validator_list_address(&builder.state);
            if let Some(validator_list) = client.get_account_retrying(&validator_list_address)? {
                if !cli_common::marinade_finance::check_id(&validator_list.owner) {
                    error!(
                        "Wrong validator list {} owner {}",
                        State::default_validator_list_address(&builder.state),
                        validator_list.owner
                    );
                    bail!(
                        "Wrong validator list {} owner {}",
                        State::default_validator_list_address(&builder.state),
                        validator_list.owner
                    );
                }

                if validator_list.data.len() < 8 {
                    error!(
                        "Validator list {} is < 8 bytes",
                        State::default_validator_list_address(&builder.state)
                    );
                    bail!(
                        "Validator list {} is < 8 bytes",
                        State::default_validator_list_address(&builder.state)
                    );
                }

                if validator_list.data[0..8] != [0; 8] {
                    error!(
                        "Validator list {} is not empty account",
                        State::default_validator_list_address(&builder.state)
                    );
                    bail!(
                        "Validator list {} is not empty account",
                        State::default_validator_list_address(&builder.state)
                    );
                }
                builder.use_validator_list(validator_list_address)
            } else {
                builder.create_validator_list_with_seed(self.max_validators, &rent);
            }
        }

        // Rewards fee
        builder.set_reward_fee(self.fee);

        // Reserve
        builder.init_reserve(
            client.get_system_balance_retrying(&builder.reserve_address())?,
            &rent,
        )?;

        // LP mint
        let _lp_mint = if let Some(lp_mint) = self.lp_mint {
            if let Some(account) = client.get_account_retrying(&lp_mint.as_pubkey())? {
                if account.owner != spl_token::ID {
                    error!(
                        "Wrong mSOL mint account {} owner {}",
                        &lp_mint.as_pubkey(),
                        account.owner
                    );
                    bail!(
                        "Wrong mSOL mint account {} owner {}",
                        &lp_mint.as_pubkey(),
                        account.owner
                    );
                }

                let mint = Mint::unpack_from_slice(&account.data).map_err(|_| {
                    error!(
                        "Can not parse account {} as SPL token mint",
                        &lp_mint.as_pubkey()
                    );
                    anyhow!(
                        "Can not parse account {} as SPL token mint",
                        &lp_mint.as_pubkey()
                    )
                })?;

                let lp_mint_owner = self.lp_mint_owner.map_or_else(
                    || common.fee_payer.as_keypair(),
                    |lp_mint_owner| lp_mint_owner.as_keypair(),
                );

                info!("Use LP mint {}", lp_mint.as_pubkey());
                builder.use_lp_mint(lp_mint.as_pubkey(), &mint, Some(lp_mint_owner))?;
            } else {
                info!("Create LP mint {}", lp_mint.as_pubkey());
                builder.create_lp_mint(
                    lp_mint.try_as_keypair().ok_or_else(|| {
                        error!(
                        "LP mint account {} does not exists. Please provide keypair to create it",
                        lp_mint
                    );
                        anyhow!(
                        "LP mint account {} does not exists. Please provide keypair to create it",
                        lp_mint
                    )
                    })?,
                    &rent,
                );
            }
            lp_mint.as_pubkey()
        } else {
            // Generate random mSOL mint
            let lp_mint = Arc::new(Keypair::new());
            warn!("Generate random LP mint {}", lp_mint.pubkey());
            builder.create_lp_mint(lp_mint.clone(), &rent);
            lp_mint.pubkey()
        };

        // Liq pool SOL leg
        builder.init_liq_pool_sol_leg(
            client.get_system_balance_retrying(&LiqPool::find_sol_leg_address(&builder.state).0)?,
            &rent,
        )?;

        // Liq pool mSOL leg
        {
            let msol_leg_address = builder.default_liq_pool_msol_leg_address();
            if client.check_token_account(
                &msol_leg_address,
                &msol_mint,
                Some(&builder.liq_pool_msol_leg_authority()),
            )? {
                builder.use_liq_pool_msol_leg(msol_leg_address)
            } else {
                builder.create_liq_pool_msol_leg_with_seed(&rent);
            }
        }

        /*
        // treasury SOL account
        let treasury_sol_account = if let Some(treasury_sol_account) = self.treasury_sol_account {
            info!("Init treasury SOL account {}", treasury_sol_account);
            treasury_sol_account.as_pubkey()
        } else {
            info!("Use fee payer as treasury SOL account");
            common.fee_payer.as_pubkey()
        };

        builder.init_treasury_sol_account(
            treasury_sol_account,
            client.get_system_balance_retrying(&treasury_sol_account)?,
            &rent,
        );*/

        // treasury mSOL account
        {
            let treasury_msol_authority =
                if let Some(treasury_msol_authority) = self.treasury_msol_authority {
                    treasury_msol_authority.as_pubkey()
                } else {
                    common.fee_payer.as_pubkey()
                };
            let treasury_msol_account =
                builder.default_treasury_msol_account(treasury_msol_authority);
            if client.check_token_account(
                &treasury_msol_account,
                &msol_mint,
                Some(&treasury_msol_authority),
            )? {
                info!("Use treasury mSOL account {}", treasury_msol_account);
                builder.use_treasury_msol_account(treasury_msol_account)
            } else {
                info!("Create treasury mSOL account {}", treasury_msol_account);
                builder.create_treasury_msol_account(treasury_msol_authority);
            }
        }
        builder.set_slots_for_stake_delta(self.slots_for_stake_delta);
        client.execute_transaction_sequence(builder.build(&rent).combined_sequence())?;

        Ok(())
    }
}
