use crate::{Command, Common};

use anyhow::{anyhow, bail, Result};
use cli_common::{
    instruction_helpers::InstructionHelpers, marinade_finance::stake_system::StakeSystemHelpers,
    rpc_client_helpers::RpcClientHelpers, rpc_marinade::RpcMarinade,
    transaction_builder::TransactionBuilder, transaction_helpers::TransactionBuilderHelpers,
    InputKeypair, InputPubkey,
};
use log::{error, info};

use cli_common::solana_sdk::{
    signature::Signer,
    stake::{self, state::StakeState},
    system_program,
};

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct DepositStakeAccount {
    #[structopt(
        short = "f",
        env = "FEE_PAYER",
        default_value = "~/.config/solana/id.json"
    )]
    fee_payer: InputKeypair,

    stake: InputPubkey,

    stake_authority: Option<InputKeypair>, // fee payer by default

    rent_payer: Option<InputKeypair>,
}

impl Command for DepositStakeAccount {
    fn process(self, _common: Common, marinade: RpcMarinade) -> Result<()> {
        info!("Using fee payer {}", self.fee_payer);

        let mut builder = TransactionBuilder::limited(self.fee_payer.as_keypair());

        info!("Delegating stake account {}", &self.stake);
        let stake_account = marinade
            .client
            .get_account_retrying(&self.stake.as_pubkey())?
            .ok_or_else(|| {
                error!("Can not find stake account {}", &self.stake);
                anyhow!("Can not find stake account {}", &self.stake)
            })?;
        if stake_account.owner != stake::program::ID {
            error!(
                "{} is not a stake account because it has owner {}",
                &self.stake, stake_account.owner
            );
            bail!(
                "{} is not a stake account because it has owner {}",
                &self.stake,
                stake_account.owner
            );
        }
        let stake: StakeState = bincode::deserialize(&stake_account.data).map_err(|e| {
            error!("Error reading stake {}: {}", &self.stake, e);
            anyhow!("Error reading stake {}: {}", &self.stake, e)
        })?;
        let current_epoch = marinade.client.get_epoch_info()?.epoch;
        let (meta, delegation) = match stake {
            StakeState::Uninitialized => {
                error!("Stake {} is uninitialized", &self.stake);
                bail!("Stake {} is uninitialized", &self.stake);
            }
            StakeState::Initialized(_) => {
                error!("Stake {} is not delegated", &self.stake);
                bail!("Stake {} is not delegated", &self.stake);
            }
            StakeState::Stake(meta, stake) => {
                if stake.delegation.deactivation_epoch != std::u64::MAX {
                    error!("Stake {} is cooling down", &self.stake);
                    bail!("Stake {} is cooling down", &self.stake);
                }
                if stake.delegation.activation_epoch >= current_epoch {
                    error!(
                        "Stake {} is too young. Please wait at least 1 epoch",
                        &self.stake
                    );
                    bail!(
                        "Stake {} is too young. Please wait at least 1 epoch",
                        &self.stake
                    );
                }
                (meta, stake.delegation)
            }
            StakeState::RewardsPool => {
                error!("Stake {} is rewards pool", &self.stake);
                bail!("Stake {} is rewards pool", &self.stake);
            }
        };

        let marinade_staker = marinade.state.stake_deposit_authority();
        let marinade_withdrawer = marinade.state.stake_withdraw_authority();

        if meta.authorized.staker == marinade_staker
            || meta.authorized.withdrawer == marinade_withdrawer
        {
            error!(
                "Stake {} already under marinade control",
                self.stake.as_pubkey()
            );
            bail!(
                "Stake {} already under marinade control",
                self.stake.as_pubkey()
            );
        }

        let input_authority = if let Some(authority) = self.stake_authority {
            info!("Using stake authority {}", authority);
            authority.as_keypair()
        } else {
            info!("Using fee payer as stake authority");
            self.fee_payer.as_keypair()
        };

        if input_authority.pubkey() != meta.authorized.withdrawer {
            // TODO: multisig
            error!(
                "Stake authority {} is invalid. Expected to be stake withdrawer {}",
                input_authority.pubkey(),
                meta.authorized.withdrawer
            );
            bail!(
                "Stake authority {} is invalid. Expected to be stake withdrawer {}",
                input_authority.pubkey(),
                meta.authorized.withdrawer
            );
        }
        let (validator_list, max_validators) = marinade.validator_list()?;

        let validator_index = if let Some(index) = validator_list
            .iter()
            .position(|validator| validator.validator_account == delegation.voter_pubkey)
        {
            index as u32
        } else {
            if validator_list.len() as u32 >= max_validators {
                error!("Validator list overflow");
                bail!("Validator list overflow");
            }
            validator_list.len() as u32
        };

        // TODO: separate fee payer from user wallet
        // find or create the associated (canonical) msol token account for the user
        let user_msol_account = builder.get_or_create_associated_token_account(
            marinade.client.clone(),
            &self.fee_payer.as_pubkey(),
            &marinade.state.msol_mint,
            "user mSOL",
        )?;

        let rent_payer = if let Some(rent_payer) = self.rent_payer {
            info!("Using rent_payer = {}", rent_payer);
            rent_payer.as_keypair()
        } else {
            info!("Using fee payer as rent payer");
            self.fee_payer.as_keypair()
        };

        if let Some(account) = marinade.client.get_account_retrying(&rent_payer.pubkey())? {
            if account.owner != system_program::ID {
                error!(
                    "Rent payer {} must be a system account",
                    rent_payer.pubkey()
                );
                bail!(
                    "Rent payer {} must be a system account",
                    rent_payer.pubkey()
                );
            }
        }

        builder.deposit_stake_account(
            &marinade.state,
            self.stake.as_pubkey(),
            input_authority,
            user_msol_account,
            validator_index,
            delegation.voter_pubkey,
            rent_payer,
        );

        marinade
            .client
            .execute_transaction_sequence(builder.combined_sequence())?;

        Ok(())
    }
}
