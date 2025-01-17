use marinade_finance_offchain_sdk::{solana_sdk, WithKey};

use std::sync::Arc;

use anyhow::bail;
use marinade_finance_offchain_sdk::anchor_lang::{
    prelude::Pubkey, AccountDeserialize, AnchorDeserialize,
};

use marinade_finance_offchain_sdk::marinade_finance::{
    stake_system::StakeRecord, validator_system::ValidatorRecord, State,
};
use marinade_finance_offchain_sdk::solana_sdk::{clock::Clock, stake::state::StakeState};
use solana_client::rpc_client::RpcClient;

use crate::rpc_client_helpers::RpcClientHelpers;

pub struct RpcMarinade {
    pub client: Arc<RpcClient>,
    pub state: WithKey<State>,
}

impl RpcMarinade {
    pub fn new(client: Arc<RpcClient>, instance_pubkey: &Pubkey) -> anyhow::Result<Self> {
        let state_account_data = client.get_account_data_retrying(instance_pubkey)?;
        Ok(Self {
            client,
            state: WithKey::<State>::new(
                AccountDeserialize::try_deserialize(&mut state_account_data.as_slice())?,
                *instance_pubkey,
            ),
        })
    }

    pub fn update(&mut self) -> anyhow::Result<()> {
        let state_account_data = self.client.get_account_data_retrying(&self.state.key)?;
        self.state.replace(AccountDeserialize::try_deserialize(
            &mut state_account_data.as_slice(),
        )?);
        Ok(())
    }

    pub fn validator_list(&self) -> anyhow::Result<(Vec<ValidatorRecord>, u32)> {
        let validator_list_account_data = self
            .client
            .get_account_data_retrying(self.state.validator_system.validator_list_address())?;
        let validator_record_size = self.state.validator_system.validator_record_size() as usize;

        Ok((
            (0..self.state.validator_system.validator_count())
                .map(|index| {
                    let start = 8 + index as usize * validator_record_size;
                    ValidatorRecord::deserialize(
                        &mut &validator_list_account_data[start..(start + validator_record_size)],
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
            self.state
                .validator_system
                .validator_list_capacity(validator_list_account_data.len())?,
        ))
    }

    pub fn stake_list(&self) -> anyhow::Result<(Vec<StakeRecord>, u32)> {
        let stake_list_account_data = self
            .client
            .get_account_data_retrying(self.state.stake_system.stake_list_address())?;
        let stake_record_size = self.state.stake_system.stake_record_size() as usize;
        Ok((
            (0..self.state.stake_system.stake_count())
                .map(|index| {
                    let start = 8 + index as usize * stake_record_size;
                    StakeRecord::deserialize(
                        &mut &stake_list_account_data[start..(start + stake_record_size)],
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
            self.state
                .stake_system
                .stake_list_capacity(stake_list_account_data.len())?,
        ))
    }

    /// composes a Vec<StakeInfo> from each account in stake_list
    /// StakeInfo includes {index, account data, stake & current balance }
    pub fn stakes_info(&self) -> anyhow::Result<(Vec<StakeInfo>, u32)> {
        let (stake_list, stakes_max_capacity) = self.stake_list()?;

        let mut result_vec: Vec<StakeInfo> = Vec::new();

        let to_process = stake_list.len();
        let mut processed = 0;
        // rpc.get_multiple_accounts() has a max of 100 accounts
        const BATCH_SIZE: usize = 100;
        while processed < to_process {
            result_vec.append(
                &mut self
                    .client
                    .get_multiple_accounts(
                        &stake_list
                            .iter()
                            .map(|record| record.stake_account)
                            .skip(processed)
                            .take(BATCH_SIZE)
                            .collect::<Vec<_>>(),
                    )?
                    .into_iter()
                    .enumerate()
                    .map(|(index, maybe_account)| {
                        if let Some(account) = maybe_account {
                            let stake = bincode::deserialize(&account.data)?;
                            Ok(StakeInfo {
                                index: processed as u32 + index as u32,
                                record: stake_list[processed + index],
                                stake,
                                balance: account.lamports,
                            })
                        } else {
                            bail!(
                                "Can not find account {} from stake list",
                                stake_list[processed + index].stake_account
                            );
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
            processed += BATCH_SIZE;
        }
        Ok((result_vec, stakes_max_capacity))
    }

    /// The vec is returned **reversed** meaning the last index is the first item.
    /// This is because when merging or deleting an account, the account record
    /// on the list on-chain is "removed". Removal is made by a "replace with last & list.count-=1"
    /// so in order to not invalidate already computed indexes (not processed yet),
    /// the account list must be processed from last to first, ergo, reversed.
    pub fn stakes_info_reversed(&self) -> anyhow::Result<(Vec<StakeInfo>, u32)> {
        let (mut vec, stakes_capacity) = self.stakes_info()?;
        // reverse vector (last indexes should be processed first)
        vec.reverse();
        Ok((vec, stakes_capacity))
    }

    pub fn get_clock(&self) -> anyhow::Result<Clock> {
        Ok(bincode::deserialize(
            &self
                .client
                .get_account_data_retrying(&solana_sdk::sysvar::clock::ID)?,
        )?)
    }
}

pub struct StakeInfo {
    pub index: u32,
    pub record: StakeRecord,
    pub stake: StakeState,
    pub balance: u64,
}
