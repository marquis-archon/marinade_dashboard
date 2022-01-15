use crate::Common;

use anyhow::Result;
use cli_common::marinade_finance::{ConfigMarinadeParams, Fee};
use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::{
    config_marinade, instruction_helpers::InstructionHelpers, rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::RpcMarinade, transaction_builder::TransactionBuilder, ExpandedPath, InputKeypair,
};
use log::info;

use cli_common::solana_sdk::native_token::sol_to_lamports;

use std::sync::Arc;
use std::{fs::File, io::Write};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct ConfigMarinadeOptions {
    #[structopt(short = "f")]
    rewards_fee: Option<Fee>,

    #[structopt(long)]
    slots_for_stake_delta: Option<u64>,

    #[structopt(long)]
    min_stake: Option<u64>,

    #[structopt(long)]
    min_deposit: Option<u64>,

    #[structopt(long)]
    min_withdraw: Option<u64>,

    #[structopt(long)]
    remove_cap: bool,

    #[structopt(short = "auto_add_validator")]
    auto_add_validator_enabled: Option<bool>,

    #[structopt(long)]
    staking_sol_cap: Option<f64>, // in SOL

    #[structopt(long)]
    liquidity_sol_cap: Option<f64>, //in SOL

    #[structopt(env = "MARINADE_ADMIN")]
    admin_authority: Option<InputKeypair>,

    #[structopt(short = "p")]
    propose_output: Option<ExpandedPath>,
}

impl ConfigMarinadeOptions {
    pub fn process(self, common: Common, client: Arc<RpcClient>) -> Result<()> {
        let marinade = RpcMarinade::new(client, &common.instance.as_pubkey())?;

        //check there's at least one parameter set
        if self.rewards_fee.is_none()
            && self.slots_for_stake_delta.is_none()
            && self.min_stake.is_none()
            && self.min_deposit.is_none()
            && self.min_withdraw.is_none()
            && self.staking_sol_cap.is_none()
            && self.liquidity_sol_cap.is_none()
            && self.auto_add_validator_enabled.is_none()
            && !self.remove_cap
        {
            return Err(anyhow::anyhow!("no parameters set"));
        }

        // cap: if present, convert to lamports, if not and --remove-cap => u64::MAX
        let staking_sol_cap = if self.staking_sol_cap.is_some() {
            Some(sol_to_lamports(self.staking_sol_cap.unwrap()))
        } else if self.remove_cap {
            Some(std::u64::MAX)
        } else {
            None
        };
        let liquidity_sol_cap = if self.liquidity_sol_cap.is_some() {
            Some(sol_to_lamports(self.liquidity_sol_cap.unwrap()))
        } else if self.remove_cap {
            Some(std::u64::MAX)
        } else {
            None
        };

        let params = ConfigMarinadeParams {
            rewards_fee: self.rewards_fee,
            slots_for_stake_delta: self.slots_for_stake_delta,
            min_stake: self.min_stake,
            min_deposit: self.min_deposit,
            min_withdraw: self.min_withdraw,
            staking_sol_cap,
            liquidity_sol_cap,
            auto_add_validator_enabled: self.auto_add_validator_enabled,
        };
        info!("{:?}", params);

        if let Some(propose_output) = self.propose_output {
            // Print transaction to stdout in multisig format
            use ::borsh::BorshSerialize;
            use multisig::{TransactionAccount, TransactionInstruction};

            let instruction = config_marinade(&marinade.state, params);

            info!(
                "instruction-data: {}",
                base64::encode(instruction.data.clone())
            );

            let transaction = TransactionInstruction {
                program_id: cli_common::marinade_finance::ID,
                accounts: instruction
                    .accounts
                    .iter()
                    .map(TransactionAccount::from)
                    .collect(),
                data: instruction.data,
            };

            if propose_output.to_str().unwrap() != "data" {
                File::create(propose_output.as_path())?.write_all(&transaction.try_to_vec()?)?;
                info!("tx saved in {}", propose_output);
            }
        } else {
            // Run transaction
            let mut builder = TransactionBuilder::limited(common.fee_payer.as_keypair());

            let admin_authority = if let Some(admin_authority) = &self.admin_authority {
                info!("Using admin authority {}", admin_authority);
                admin_authority.as_keypair()
            } else {
                info!("Using fee payer as admin authority");
                common.fee_payer.as_keypair()
            };

            builder.config_marinade(&marinade.state, admin_authority, params)?;

            marinade.client.execute_transaction(builder.build_one())?;
        }
        Ok(())
    }
}
