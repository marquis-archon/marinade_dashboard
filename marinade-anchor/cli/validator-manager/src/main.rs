#![cfg_attr(not(debug_assertions), deny(warnings))]

use anyhow::bail;
use cli_common::{
    init_log, log_level_opts::QuietVerbose, Cluster, ExpandedPath, InputKeypair, InputPubkey,
};

use log::{debug, error, info, LevelFilter};

use cli_common::solana_client::rpc_client::RpcClient;
use cli_common::solana_sdk::commitment_config::CommitmentConfig;

use std::{str::FromStr, sync::Arc};
use structopt::StructOpt;

pub mod add_validator;
pub mod config_validator_system;
pub mod emergency_unstake;
pub mod remove_validators;
pub mod update_scores;

use add_validator::AddValidatorOptions;
use config_validator_system::ConfigValidatorsOptions;
use emergency_unstake::EmergencyUnstakeOptions;
use remove_validators::RemoveValidatorsOptions;
use update_scores::UpdateScoresOptions;

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

    #[structopt(
        short = "i",
        env = "MARINADE_INSTANCE",
        default_value = "auto" //select default instance based on cluster
        // other possible values:
        //default_value = "~/.config/mardmin/instance.json"
        //default_value = "9tA9pzAZWimw2EMZgMjmUwzB2qPKrHhFNaC2ZvCrReeh"
    )]
    instance: InputPubkey,

    #[structopt(short = "s", long = "simulate", help = "only simulate transaction")]
    simulate: bool,

    validator_manager_authority: Option<InputKeypair>,
    rent_payer: Option<InputKeypair>,

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

#[derive(Debug, StructOpt)]
enum MardminCommand {
    AddValidator(AddValidatorOptions),
    RemoveValidators(RemoveValidatorsOptions),
    UpdateScores(UpdateScoresOptions),
    ConfigValidators(ConfigValidatorsOptions),
    EmergencyUnstake(EmergencyUnstakeOptions),
}

fn main() -> anyhow::Result<()> {
    let mut params = Params::from_args();

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
    let cluster = Cluster::from_url(&cli_config.json_rpc_url);
    info!(
        "Cluster: {:?}, commitment: {}",
        cluster, &cli_config.commitment
    );

    // if instance is "auto" use default per cluster
    if let InputPubkey::Auto = params.common.instance {
        params.common.instance = InputPubkey::Pubkey(cluster.default_instance());
    };
    info!("Instance: {:?}", params.common.instance);

    debug!("Solana config: {:?}", cli_config);

    let client = Arc::new(RpcClient::new_with_commitment(
        cli_config.json_rpc_url,
        CommitmentConfig::from_str(&cli_config.commitment).unwrap(),
    ));

    info!("Using fee payer {}", params.common.fee_payer);
    Ok(match params.command {
        MardminCommand::AddValidator(options) => options.process(params.common, client),
        MardminCommand::RemoveValidators(options) => options.process(params.common, client),
        MardminCommand::UpdateScores(options) => options.process(params.common, client, cluster),
        MardminCommand::ConfigValidators(options) => options.process(params.common, client),
        MardminCommand::EmergencyUnstake(options) => options.process(params.common, client),
    }?)
}
