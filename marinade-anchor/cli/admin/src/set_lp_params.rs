use crate::Common;

use anyhow::anyhow;
use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::solana_sdk::native_token::{lamports_to_sol, sol_to_lamports};
use cli_common::{
    instruction_helpers::InstructionHelpers, marinade_finance::Fee,
    rpc_client_helpers::RpcClientHelpers, rpc_marinade::RpcMarinade, set_lp_params,
    transaction_builder::TransactionBuilder, ExpandedPath, InputKeypair,
};
use log::info;

use std::fs::File;
use std::io::Write;

use std::sync::Arc;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct SetLpParamsOptions {
    #[structopt(short = "n")]
    min_fee: Option<Fee>,

    #[structopt(short = "x")]
    max_fee: Option<Fee>,

    #[structopt(short = "t", help = "liquidity target in SOL")]
    liquidity_target: Option<f64>,

    #[structopt(env = "MARINADE_ADMIN")]
    admin_authority: Option<InputKeypair>,

    #[structopt(short = "p")]
    propose_output: Option<ExpandedPath>,
}

impl SetLpParamsOptions {
    pub fn process(self, common: Common, client: Arc<RpcClient>) -> anyhow::Result<()> {
        let marinade = RpcMarinade::new(client, &common.instance.as_pubkey())?;

        if self.min_fee.is_none() && self.max_fee.is_none() && self.liquidity_target.is_none() {
            info!("missing parameters");
            return Err(anyhow!("missing parameters"));
        }
        // take command line options or existent values
        let min_fee = self.min_fee.unwrap_or(marinade.state.liq_pool.lp_min_fee);
        let max_fee = self.max_fee.unwrap_or(marinade.state.liq_pool.lp_max_fee);
        let liquidity_target = self
            .liquidity_target
            .unwrap_or(lamports_to_sol(marinade.state.liq_pool.lp_liquidity_target));

        info!(
            "Set LP min_fee = {}, max_fee = {}, liquidity_target = {} SOL",
            min_fee, max_fee, liquidity_target
        );

        if let Some(propose_output) = self.propose_output {
            // Print transaction to stdout in multisig format
            use ::borsh::BorshSerialize;
            use multisig::{TransactionAccount, TransactionInstruction};

            let instruction = set_lp_params(
                &marinade.state,
                min_fee,
                max_fee,
                sol_to_lamports(liquidity_target),
            );
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

            builder.set_lp_params(
                &marinade.state,
                admin_authority,
                min_fee,
                max_fee,
                sol_to_lamports(liquidity_target),
            )?;

            marinade.client.execute_transaction(builder.build_one())?;
        }
        Ok(())
    }
}
