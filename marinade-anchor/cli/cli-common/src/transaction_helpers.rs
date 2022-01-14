use crate::{rpc_client_helpers::RpcClientHelpers, transaction_builder::TransactionBuilder};
use log::info;
use marinade_finance_offchain_sdk::solana_sdk::pubkey::Pubkey;
use marinade_finance_offchain_sdk::spl_associated_token_account::get_associated_token_address;
use solana_client::rpc_client::RpcClient;

pub trait TransactionBuilderHelpers {
    fn get_or_create_associated_token_account(
        &mut self,
        client: impl AsRef<RpcClient>,
        owner: &Pubkey,
        mint: &Pubkey,
        name: &str,
    ) -> anyhow::Result<Pubkey>;
}

impl TransactionBuilderHelpers for TransactionBuilder {
    //-------------------------------------
    /// helper fn, finds or creates the associated (canonical) token account
    /// https://spl.solana.com/associated-token-account
    /// given a main account & a mint. If the account is not found, adds the creation instruction to the tx
    //-------------------------------------
    fn get_or_create_associated_token_account(
        &mut self,
        client: impl AsRef<RpcClient>,
        owner: &Pubkey,
        mint: &Pubkey,
        name: &str,
    ) -> anyhow::Result<Pubkey> {
        let account = get_associated_token_address(owner, mint);
        if client.as_ref().get_account_retrying(&account)?.is_some() {
            info!("Using associated {} account {}", name, account);
        } else {
            info!("Creating associated {} account {}", name, account);
            let actual_account = self.create_associated_token_account(&owner, &mint, name)?;
            assert_eq!(actual_account, account);
        }
        Ok(account)
    }
}
