#![cfg_attr(not(debug_assertions), deny(warnings))]

use std::{str::FromStr, sync::Arc};

use anyhow::{anyhow, bail};

use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::solana_sdk::commitment_config::CommitmentConfig;
use cli_common::{
    init_log, log_level_opts::QuietVerbose, rpc_marinade::RpcMarinade, Cluster, ExpandedPath,
    InputPubkey,
};
use log::{debug, error, info, LevelFilter};
use marstat::Farms;
use postgres::NoTls;

use structopt::StructOpt;

pub mod balances;
use balances::BalancesSample;

#[derive(Debug, StructOpt)]
struct Params {
    #[structopt(short = "c", default_value = "~/.config/solana/cli/config.yml")]
    config_file: ExpandedPath,

    #[structopt(flatten)]
    verbose: QuietVerbose,

    #[structopt(short = "i", env = "MARINADE_INSTANCE", default_value = "auto")]
    instance: InputPubkey,

    postgres_config_file: ExpandedPath,
}

fn main() -> anyhow::Result<()> {
    let mut params = Params::from_args();

    init_log(params.verbose.get_level_filter(LevelFilter::Info));

    debug!("marstat {:?}", params);

    let cli_config = match solana_cli_config::Config::load(&params.config_file.to_string()) {
        Ok(cli_config) => cli_config,
        Err(err) => {
            error!(
                "Solana CLI config {} reading error: {}",
                params.config_file.to_string(),
                err
            );
            bail!(
                "Solana CLI config {} reading error: {}",
                params.config_file.to_string(),
                err
            );
        }
    };
    debug!("Solana config: {:?}", cli_config);

    let cluster = Cluster::from_url(&cli_config.json_rpc_url);
    info!(
        "Cluster: {:?}, commitment: {}",
        cluster, &cli_config.commitment
    );
    // if instance is "auto" use default per cluster
    if let InputPubkey::Auto = params.instance {
        params.instance = InputPubkey::Pubkey(cluster.default_instance());
    };
    info!("ProgramId: {:?}", cli_common::marinade_finance::ID);
    info!("Instance : {:?}", params.instance);

    let solana_client = Arc::new(RpcClient::new_with_commitment(
        cli_config.json_rpc_url,
        CommitmentConfig::from_str(&cli_config.commitment).unwrap(),
    ));

    let mut marinade = RpcMarinade::new(solana_client, &params.instance.as_pubkey())?;

    let postgres_config = postgres::Config::from_str(
        &std::fs::read_to_string(params.postgres_config_file.as_path()).map_err(|e| {
            anyhow!(
                "Error reading postgres config file {}: {}",
                params.postgres_config_file.as_path().display(),
                e
            )
        })?,
    )
    .map_err(|e| {
        anyhow!(
            "Error parsing postgres config from file {}: {}",
            params.postgres_config_file.as_path().display(),
            e
        )
    })?;
    let mut postgres_client = postgres_config.connect(NoTls)?;

    let balances_sample = BalancesSample::from_blockchain(
        marinade.client.as_ref(),
        &mut marinade.state,
        CommitmentConfig::from_str(&cli_config.commitment).unwrap(),
    )?;

    balances_sample.db_save(&mut postgres_client)?;

    let farms = Farms::from_blockchain(
        marinade.client.as_ref(),
        CommitmentConfig::from_str(&cli_config.commitment).unwrap(),
    )?;

    for farm in farms {
        farm.db_save(&mut postgres_client)?;
    }

    Ok(())
}
