#![cfg_attr(not(debug_assertions), deny(warnings))]

use std::sync::Arc;

use anyhow::bail;
use cli_common::{
    init_log, log_level_opts::QuietVerbose, rpc_marinade::RpcMarinade, Cluster, ExpandedPath,
    InputKeypair, InputPubkey,
};
use enum_dispatch::enum_dispatch;
use log::{debug, error, info, LevelFilter};

use std::str::FromStr;

use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::solana_sdk::commitment_config::CommitmentConfig;
use structopt::StructOpt;

pub mod add_remove_liquidity;
pub mod claim;
pub mod deposit_stake_account;
pub mod liquid_unstake;
pub mod order_unstake;
pub mod show;
pub mod stake;

use add_remove_liquidity::*;
use claim::Claim;
use deposit_stake_account::DepositStakeAccount;
use liquid_unstake::LiquidUnstake;
use order_unstake::OrderUnstake;
use show::Show;
use stake::Stake;

#[derive(Debug, StructOpt)]
pub struct Common {
    #[structopt(short = "c", default_value = "~/.config/solana/cli/config.yml")]
    config_file: ExpandedPath,

    #[structopt(flatten)]
    verbose: QuietVerbose,

    #[structopt(
        short = "i",
        env = "MARINADE_INSTANCE",
        default_value = "auto"
        //default_value = "~/.config/mardmin/instance.json"
    )]
    instance: InputPubkey,
}

#[enum_dispatch]
pub trait Command {
    fn process(self, common: Common, marinade: RpcMarinade) -> anyhow::Result<()>;
}

#[enum_dispatch(Command)]
#[derive(Debug, StructOpt)]
enum SmartPoolCommand {
    Show,
    Stake,
    LiquidUnstake,
    AddLiquidity,
    RemoveLiquidity,
    DepositStakeAccount,
    OrderUnstake,
    Claim,
}

#[derive(Debug, StructOpt)]
struct Params {
    #[structopt(flatten)]
    common: Common,
    #[structopt(subcommand)]
    command: SmartPoolCommand,
}

fn main() -> anyhow::Result<()> {
    let mut params = Params::from_args();

    init_log(params.common.verbose.get_level_filter(LevelFilter::Info));

    debug!("smartpool {:?}", params);

    let cli_config = match solana_cli_config::Config::load(&params.common.config_file.to_string()) {
        Ok(cli_config) => cli_config,
        Err(err) => {
            error!(
                "Solana CLI config {} reading error: {}",
                params.common.config_file.to_string(),
                err
            );
            bail!(
                "Solana CLI config {} reading error: {}",
                params.common.config_file.to_string(),
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
    if let InputPubkey::Auto = params.common.instance {
        params.common.instance = InputPubkey::Pubkey(cluster.default_instance());
    };
    info!("ProgramId: {:?}", cli_common::marinade_finance::ID);
    info!("Instance : {:?}", params.common.instance);

    let client = Arc::new(RpcClient::new_with_commitment(
        cli_config.json_rpc_url,
        CommitmentConfig::from_str(&cli_config.commitment).unwrap(),
    ));

    let marinade = RpcMarinade::new(client, &params.common.instance.as_pubkey())?;

    params.command.process(params.common, marinade)
}
