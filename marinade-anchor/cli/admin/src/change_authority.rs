use crate::Common;

use anyhow::Result;
use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::{
    change_authority, instruction_helpers::InstructionHelpers,
    marinade_finance::ChangeAuthorityData, rpc_client_helpers::RpcClientHelpers,
    rpc_marinade::RpcMarinade, transaction_builder::TransactionBuilder, ExpandedPath, InputKeypair,
    InputPubkey,
};
use log::info;

use std::sync::Arc;
use std::{fs::File, io::Write};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct ChangeAuthorityOptions {
    #[structopt(long)]
    new_admin: Option<InputPubkey>,

    #[structopt(long)]
    new_validator_manager: Option<InputPubkey>,

    #[structopt(long)]
    new_operational_sol_account: Option<InputPubkey>,

    #[structopt(long)]
    new_treasury_msol_account: Option<InputPubkey>,

    #[structopt(env = "MARINADE_ADMIN")]
    current_admin_authority: Option<InputKeypair>,

    #[structopt(short = "p")]
    propose_output: Option<ExpandedPath>,
}

impl ChangeAuthorityOptions {
    pub fn process(self, common: Common, client: Arc<RpcClient>) -> Result<()> {
        info!("change authority");

        let marinade = RpcMarinade::new(client, &common.instance.as_pubkey())?;

        //check there's at least one parameter set
        if self.new_admin.is_none()
            && self.new_validator_manager.is_none()
            && self.new_operational_sol_account.is_none()
            && self.new_treasury_msol_account.is_none()
        {
            return Err(anyhow::anyhow!("no parameters set"));
        }

        let data = ChangeAuthorityData {
            admin: self.new_admin.as_ref().map(InputPubkey::as_pubkey),
            validator_manager: self
                .new_validator_manager
                .as_ref()
                .map(InputPubkey::as_pubkey),
            operational_sol_account: self
                .new_operational_sol_account
                .as_ref()
                .map(InputPubkey::as_pubkey),
            treasury_msol_account: self
                .new_treasury_msol_account
                .as_ref()
                .map(InputPubkey::as_pubkey),
        };

        if let Some(propose_output) = self.propose_output {
            // Print transaction to stdout in multisig format
            use ::borsh::BorshSerialize;
            use multisig::{TransactionAccount, TransactionInstruction};

            let instruction = change_authority(&marinade.state, data);

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

            let admin_authority = if let Some(admin_authority) = &self.current_admin_authority {
                info!("Using current admin authority {}", admin_authority);
                admin_authority.as_keypair()
            } else {
                info!("Using fee payer as current admin authority");
                common.fee_payer.as_keypair()
            };

            builder.change_authority(&marinade.state, admin_authority, data)?;

            marinade.client.execute_transaction(builder.build_one())?;
        }
        Ok(())
    }
}
