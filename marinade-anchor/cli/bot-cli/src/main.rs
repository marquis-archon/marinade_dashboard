#![cfg_attr(not(debug_assertions), deny(warnings))]

use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::bail;
use log::{debug, error, info, LevelFilter};
use structopt::StructOpt;

use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::solana_sdk::commitment_config::CommitmentConfig;

use cli_common::{
    init_log, log_level_opts::QuietVerbose, rpc_marinade::RpcMarinade,
    transaction_builder::TransactionBuilder, Cluster, ExpandedPath, InputKeypair, InputPubkey,
};

pub mod do_work;
pub mod merge_stakes;
pub mod stake_delta;
pub mod update_price;

use do_work::*;
use stake_delta::*;
use update_price::*;

#[allow(dead_code)]
#[derive(Debug, StructOpt)]
pub struct Common {
    #[structopt(short = "c", default_value = "~/.config/solana/cli/config.yml")]
    config_file: ExpandedPath,

    #[structopt(flatten)]
    verbose: QuietVerbose,

    #[structopt(
        short = "f",
        env = "FEE_PAYER",
        default_value = "~/.config/solana/id.json"
    )]
    fee_payer: InputKeypair,

    #[structopt(
        short = "i",
        env = "MARINADE_INSTANCE",
        default_value = "auto",
        help = "instance"
    )]
    instance: InputPubkey,

    #[structopt(
        short = "l",
        name = "limit",
        default_value = "0",
        help = "execute at most n transactions"
    )]
    limit: u32,

    #[structopt(short = "s", long = "simulate", help = "only simulate transaction")]
    simulate: bool,

    #[structopt(subcommand)] // Note that we mark a field as a subcommand
    cmd: BotCliCommand,
}

#[derive(Debug, StructOpt)]
struct CliArgs {
    #[structopt(flatten)]
    common: Common,
    #[structopt(subcommand)]
    command: BotCliCommand,
}

#[derive(StructOpt, Debug)]
enum BotCliCommand {
    StakeDelta(StakeDeltaOptions),
    UpdatePrice(UpdatePriceOptions),
    MergeStakes,
    DoWork(DoWorkOptions),
}

fn main() -> anyhow::Result<()> {
    let mut cli = CliArgs::from_args();

    init_log(cli.common.verbose.get_level_filter(LevelFilter::Info));

    debug!("bot-cli {:?}", cli);

    let cli_config = match solana_cli_config::Config::load(&cli.common.config_file.to_string()) {
        Ok(cli_config) => cli_config,
        Err(err) => {
            error!(
                "Solana CLI config {} reading error: {}",
                cli.common.config_file.to_string(),
                err
            );
            bail!(
                "Solana CLI config {} reading error: {}",
                cli.common.config_file.to_string(),
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

    let client = Arc::new(RpcClient::new_with_commitment(
        cli_config.json_rpc_url,
        CommitmentConfig::from_str(&cli_config.commitment).unwrap(),
    ));

    // user can pass -i pubkey || -i ~/.config/path/to/keyFile.json
    // if instance is "auto" use default per cluster
    if let InputPubkey::Auto = cli.common.instance {
        cli.common.instance = InputPubkey::Pubkey(cluster.default_instance());
    };

    info!("ProgramId: {:?}", cli_common::marinade_finance::ID);
    info!("Instance : {:?}", cli.common.instance);
    info!("Using fee payer {}", &cli.common.fee_payer.as_pubkey());
    let mut marinade = RpcMarinade::new(client, &cli.common.instance.as_pubkey())?;
    let mut builder = TransactionBuilder::limited(cli.common.fee_payer.as_keypair());

    match cli.command {
        BotCliCommand::StakeDelta(x) => {
            x.process(
                &cli.common,
                &mut marinade,
                &mut builder,
                &SystemTime::now(),
                10 * 60,
            )?;
        }
        BotCliCommand::UpdatePrice(x) => {
            x.process(&cli.common, &marinade, &mut builder)?;
        }
        BotCliCommand::MergeStakes => {
            merge_stakes::process(&cli.common, &marinade, &mut builder, 0)?
        }
        BotCliCommand::DoWork(x) => x.process(&cli.common, &mut marinade, &mut builder)?,
    }
    Ok(())
}
