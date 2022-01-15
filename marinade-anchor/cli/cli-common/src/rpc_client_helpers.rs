use anyhow::{anyhow, bail};
use log::{debug, error, info, warn};
use marinade_finance_offchain_sdk::anchor_lang::prelude::Pubkey;
use marinade_finance_offchain_sdk::solana_sdk::{
    account::Account, program_pack::Pack, system_program,
};
use marinade_finance_offchain_sdk::spl_token;
use marinade_finance_offchain_sdk::spl_token::state::{Account as Token, Mint};
use marinade_finance_offchain_sdk::transaction_builder::PreparedTransaction;
use solana_client::{
    client_error::{ClientError, ClientErrorKind},
    rpc_client::RpcClient,
    rpc_request::{RpcError, RpcResponseErrorData},
    rpc_response::RpcSimulateTransactionResult,
};

pub trait RpcClientHelpers {
    fn get_account_retrying(&self, account_pubkey: &Pubkey)
        -> Result<Option<Account>, ClientError>;
    fn get_account_data_retrying(&self, account_pubkey: &Pubkey) -> anyhow::Result<Vec<u8>>;
    fn get_system_balance_retrying(&self, account_pubkey: &Pubkey) -> anyhow::Result<u64>;

    fn check_mint_account(
        &self,
        account_pubkey: &Pubkey,
        authority: &Pubkey,
        must_have_0_supply: bool,
    ) -> anyhow::Result<bool>;

    fn check_token_account(
        &self,
        account_pubkey: &Pubkey,
        mint: &Pubkey,
        authority: Option<&Pubkey>,
    ) -> anyhow::Result<bool>;

    fn execute_transaction(&self, transaction: PreparedTransaction) -> Result<(), ClientError>;
    fn execute_transaction_sequence(
        &self,
        transactions: impl IntoIterator<Item = PreparedTransaction>,
    ) -> Result<(), ClientError> {
        for transaction in transactions.into_iter() {
            self.execute_transaction(transaction)?;
        }

        Ok(())
    }

    fn simulate_prepared_transaction(
        &self,
        transaction: PreparedTransaction,
    ) -> Result<(), ClientError>;
    fn simulate_transaction_sequence(
        &self,
        transactions: impl IntoIterator<Item = PreparedTransaction>,
    ) -> Result<(), ClientError> {
        for transaction in transactions.into_iter() {
            self.simulate_prepared_transaction(transaction)?;
        }

        Ok(())
    }

    fn process_transaction(
        &self,
        simulation: bool,
        transaction: PreparedTransaction,
    ) -> Result<(), ClientError> {
        if simulation {
            self.simulate_prepared_transaction(transaction)
        } else {
            self.execute_transaction(transaction)
        }
    }

    // execute or simulate according to flag
    fn process_transaction_sequence(
        &self,
        simulation: bool,
        transactions: impl IntoIterator<Item = PreparedTransaction>,
    ) -> Result<(), ClientError> {
        for transaction in transactions.into_iter() {
            self.process_transaction(simulation, transaction)?;
        }

        Ok(())
    }
}

impl RpcClientHelpers for RpcClient {
    fn get_account_retrying(
        &self,
        account_pubkey: &Pubkey,
    ) -> Result<Option<Account>, ClientError> {
        Ok(loop {
            match self.get_account_with_commitment(account_pubkey, self.commitment()) {
                Ok(account) => break account,
                Err(err) => warn!("RPC error {}. Retrying", err),
            }
        }
        .value)
    }

    fn get_account_data_retrying(&self, account_pubkey: &Pubkey) -> anyhow::Result<Vec<u8>> {
        if let Some(account) = self.get_account_retrying(account_pubkey)? {
            Ok(account.data)
        } else {
            error!("Can not find account {}", account_pubkey);
            bail!("Can not find account {}", account_pubkey);
        }
    }

    fn get_system_balance_retrying(&self, account_pubkey: &Pubkey) -> anyhow::Result<u64> {
        if let Some(account) = self.get_account_retrying(account_pubkey)? {
            if account.owner != system_program::ID {
                error!(
                    "Account {} must belongs to system. But owner is {}",
                    account_pubkey, account.owner
                );
                bail!(
                    "Account {} must belongs to system. But owner is {}",
                    account_pubkey,
                    account.owner
                );
            }
            Ok(account.lamports)
        } else {
            Ok(0)
        }
    }

    fn check_mint_account(
        &self,
        account_pubkey: &Pubkey,
        authority: &Pubkey,
        must_have_0_supply: bool,
    ) -> anyhow::Result<bool> {
        if let Some(account) = self.get_account_retrying(account_pubkey)? {
            if account.owner != spl_token::ID {
                error!(
                    "Wrong SPL mint account {} owner {}",
                    account_pubkey, account.owner
                );
                bail!(
                    "Wrong SPL mint account {} owner {}",
                    account_pubkey,
                    account.owner
                );
            }

            let mint = Mint::unpack_from_slice(&account.data).map_err(|_| {
                error!("Can not parse account {} as SPL token mint", account_pubkey);
                anyhow!("Can not parse account {} as SPL token mint", account_pubkey)
            })?;

            if !mint.mint_authority.contains(authority) {
                error!(
                    "Wrong mint authority {}. Must be {}. Mint:{}",
                    mint.mint_authority.unwrap_or_default(),
                    authority,
                    account_pubkey
                );
                bail!(
                    "Wrong mint authority {}. Must be {}. Mint:{}",
                    mint.mint_authority.unwrap_or_default(),
                    authority,
                    account_pubkey
                );
            }

            if mint.freeze_authority.is_some() {
                error!(
                    "Freeze authority of mint {} must not be set",
                    account_pubkey
                );
                bail!(
                    "Freeze authority of mint {} must not be set",
                    account_pubkey
                );
            }

            if must_have_0_supply && mint.supply > 0 {
                error!("Mint {} must have 0 supply", account_pubkey);
                bail!("Mint {} must have 0 supply", account_pubkey);
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn check_token_account(
        &self,
        account_pubkey: &Pubkey,
        mint: &Pubkey,
        authority: Option<&Pubkey>,
    ) -> anyhow::Result<bool> {
        if let Some(account) = self.get_account_retrying(account_pubkey)? {
            if account.owner != spl_token::ID {
                error!(
                    "Wrong SPL mint account {} owner {}",
                    account_pubkey, account.owner
                );
                bail!(
                    "Wrong SPL mint account {} owner {}",
                    account_pubkey,
                    account.owner
                );
            }

            let token = Token::unpack_from_slice(&account.data).map_err(|_| {
                error!("Can not parse account {} as SPL token", account_pubkey);
                anyhow!("Can not parse account {} as SPL token", account_pubkey)
            })?;

            if token.mint != *mint {
                error!(
                    "Wrong token account {} mint {}. Expected {}",
                    account_pubkey, token.mint, mint
                );
                bail!(
                    "Wrong token account {} mint {}. Expected {}",
                    account_pubkey,
                    token.mint,
                    mint
                );
            }

            if let Some(authority) = authority {
                if token.owner != *authority {
                    error!(
                        "Wrong token account {} authority {}. Expected {}",
                        account_pubkey, token.owner, authority
                    );
                    bail!(
                        "Wrong token account {} authority {}. Expected {}",
                        account_pubkey,
                        token.owner,
                        authority
                    );
                }
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn execute_transaction(&self, mut transaction: PreparedTransaction) -> Result<(), ClientError> {
        debug!("Run transaction with instructions:");
        for (index, description) in transaction.instruction_descriptions.iter().enumerate() {
            debug!("Instruction #{}: {}", index, description);
        }
        match self.send_and_confirm_transaction_with_spinner(
            transaction.sign(self.get_recent_blockhash()?.0)?,
        ) {
            Ok(signature) => info!("Transaction {}", signature),
            Err(err) => {
                error!("Transaction error: {}", err);
                if let ClientErrorKind::RpcError(RpcError::RpcResponseError {
                    data:
                        RpcResponseErrorData::SendTransactionPreflightFailure(
                            RpcSimulateTransactionResult {
                                err: _,
                                logs: Some(logs),
                                accounts: _,
                            },
                        ),
                    ..
                }) = err.kind()
                {
                    for log in logs {
                        error!("Log: {}", log);
                    }
                    return Err(err);
                }
            }
        }
        Ok(())
    }

    fn simulate_prepared_transaction(
        &self,
        mut transaction: PreparedTransaction,
    ) -> Result<(), ClientError> {
        debug!("simulate transaction with instructions:");
        for (index, description) in transaction.instruction_descriptions.iter().enumerate() {
            debug!("Instruction #{}: {}", index, description);
        }
        match self.simulate_transaction(transaction.sign(self.get_recent_blockhash()?.0)?) {
            Ok(result) => {
                if let Some(logs) = &result.value.logs {
                    for log in logs {
                        debug!("Log: {}", log);
                    }
                }
                if result.value.err.is_some() {
                    info!("Transaction ERR {:?}", result);
                } else {
                    info!("Transaction Ok");
                }
            }
            Err(err) => {
                error!("Transaction error: {}", err);
                if let ClientErrorKind::RpcError(RpcError::RpcResponseError {
                    data:
                        RpcResponseErrorData::SendTransactionPreflightFailure(
                            RpcSimulateTransactionResult {
                                err: _,
                                logs: Some(logs),
                                accounts: _,
                            },
                        ),
                    ..
                }) = err.kind()
                {
                    for log in logs {
                        info!("Log: {}", log);
                    }
                    return Err(err);
                }
            }
        }
        Ok(())
    }

    // commented, it's already in the trait
    // fn execute_transaction_sequence(
    //     &self,
    //     transactions: impl IntoIterator<Item = PreparedTransaction>,
    // ) -> Result<(), ClientError> {
    //     for transaction in transactions.into_iter() {
    //         self.execute_transaction(transaction)?;
    //     }

    //     Ok(())
    // }
}
