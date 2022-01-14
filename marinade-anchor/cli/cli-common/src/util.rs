use std::env;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use solana_sdk::{
    instruction::Instruction,
    program_pack::Pack,
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signer},
    system_instruction, system_program,
};

use solana_account_decoder::{parse_token::TokenAccountType, UiAccountData, UiAccountEncoding};

use solana_client::{
    rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType},
    rpc_request::TokenAccountsFilter,
    rpc_response::{Response, RpcKeyedAccount},
};

use anchor_lang::AccountDeserialize;
use anyhow::{anyhow, bail, Result};

use marinade_finance::state::State;
use marinade_finance_sdk::WithKey;
/*
/// read state from instance pubkey using RPC client
pub fn get_state(client: &mut RpcClient, instance_pubkey: &Pubkey) -> Result<WithKey<State>> {
    let state_account_data = client.get_account_data(instance_pubkey)?;
    Ok(WithKey::<State>::new(
        AccountDeserialize::try_deserialize(&mut state_account_data.as_slice())?,
        *instance_pubkey,
    ))
}*/
/*
// create a SPL-token account
pub fn create_token_account(
    mint: &Pubkey,
    owner: &Pubkey,
    min_account_balance: u64,
    out_instructions: &mut Vec<Instruction>,
) -> Keypair {
    let keypair = Keypair::new();

    // Account for tokens not specified, creating one
    println!(
        "Creating token account {} mint:{} owner:{}",
        keypair.pubkey(),
        mint,
        owner
    );

    out_instructions.extend(vec![
        // Creating new account
        system_instruction::create_account(
            &owner,
            &keypair.pubkey(),
            min_account_balance,
            spl_token::state::Account::LEN as u64,
            &spl_token::id(),
        ),
        // Initialize token account
        spl_token::instruction::initialize_account(
            &spl_token::id(),
            &keypair.pubkey(),
            mint,
            &owner,
        )
        .unwrap(),
    ]);

    keypair
}

//-------------------------------------
// helper fn, finds accounts by owner & mint
// Note: if you need a toke account, it's better to use spl_associated_token_account::get_associated_token_address(owner, mint)
//-------------------------------------
pub fn get_accounts_by_mint(
    client: &RpcClient,
    owner: Pubkey,
    mint: Pubkey,
) -> Vec<RpcKeyedAccount> {
    client
        .get_token_accounts_by_owner(&owner, TokenAccountsFilter::Mint(mint))
        .unwrap()
}

//gets token amount from UiAccountData
pub fn token_account_amount(data: UiAccountData) -> u64 {
    if let UiAccountData::Json(parsed_account) = data {
        if parsed_account.program != "spl-token" {
            0
        } else {
            match serde_json::from_value(parsed_account.parsed) {
                Ok(TokenAccountType::Account(ui_token_account)) => ui_token_account
                    .token_amount
                    .amount
                    .parse::<u64>()
                    .unwrap_or_default(),
                Ok(_) => 0,
                Err(_err) => 0,
            }
        }
    } else {
        0
    }
}

pub fn check_account_existence(
    client: &RpcClient,
    account_pubkey: &Pubkey,
    owner: &Pubkey,
) -> anyhow::Result<bool> {
    let Response {
        context: _,
        value: account,
    } = loop {
        match client.get_account_with_commitment(&account_pubkey, client.commitment()) {
            Ok(account) => break account,
            Err(err) => println!("{}. Retrying", err),
        }
    };
    if let Some(account) = account {
        if account.owner != *owner {
            bail!(
                "Wrong account {} owner {}. Expected {}",
                account_pubkey,
                account.owner,
                owner
            );
        }
        Ok(true)
    } else {
        Ok(false)
    }
}
*/
