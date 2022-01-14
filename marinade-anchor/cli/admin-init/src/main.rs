#![cfg_attr(not(debug_assertions), deny(warnings))]

use anyhow::bail;
use cli_common::{init_log, log_level_opts::QuietVerbose, ExpandedPath, InputKeypair};
use enum_dispatch::enum_dispatch;
use log::{debug, error, info, LevelFilter};

use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::solana_sdk::commitment_config::CommitmentConfig;

use std::{str::FromStr, sync::Arc};
use structopt::StructOpt;

pub mod init;

use init::Init;

#[derive(Debug, StructOpt)]
pub struct Common {
    #[structopt(short = "c", default_value = "~/.config/solana/cli/config.yml")]
    config_file: ExpandedPath,

    #[structopt(
        short = "f",
        env = "FEE_PAYER",
        default_value = "~/.config/solana/id.json"
    )]
    fee_payer: InputKeypair,

    #[structopt(flatten)]
    verbose: QuietVerbose,
}

#[derive(Debug, StructOpt)]
struct Params {
    #[structopt(flatten)]
    common: Common,
    #[structopt(subcommand)]
    command: MardminCommand,
}

#[enum_dispatch(Command)]
#[derive(Debug, StructOpt)]
enum MardminCommand {
    Init,
}

#[enum_dispatch]
pub trait Command {
    fn process(self, common: Common, client: Arc<RpcClient>) -> anyhow::Result<()>;
}

fn main() -> anyhow::Result<()> {
    let params = Params::from_args();

    init_log(params.common.verbose.get_level_filter(LevelFilter::Info));

    debug!("mardmin {:?}", params);

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
    info!("Solana config: {:?}", cli_config);

    let client = Arc::new(RpcClient::new_with_commitment(
        cli_config.json_rpc_url,
        CommitmentConfig::from_str(&cli_config.commitment).unwrap(),
    ));

    info!("Using fee payer {}", params.common.fee_payer);
    params.command.process(params.common, client)
}
