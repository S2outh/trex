use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use clap_num::maybe_hex;
use trex_probe::{FlashConf, NetConf};

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, short, default_value = "trex.lan")]
    pub host: String,

    #[arg(long, short, default_value_t = 3000)]
    pub port: u16,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Flash, run and validate the executable on the target,
    /// then attach the logger
    Run(FlashArgs),
    /// Flash the executable on the target
    Flash(FlashArgs),
    /// Validate the current firmware on the target
    Validate,
    /// Reset the target
    Reset,
}

#[derive(Args)]
pub struct FlashArgs {
    /// path to the executable
    pub path: PathBuf,
    /// base offset to optionally check against
    #[arg(long, value_parser=maybe_hex::<u64>)]
    pub base: Option<u64>,
    /// length of active partition to optionally check against
    #[arg(long, value_parser=maybe_hex::<u64>)]
    pub max_size: Option<u64>,
}

impl From<&Cli> for NetConf {
    fn from(v: &Cli) -> Self {
        NetConf {
            host: v.host.clone(),
            port: v.port,
        }
    }
}

impl From<FlashArgs> for FlashConf {
    fn from(v: FlashArgs) -> Self {
        FlashConf {
            path: v.path,
            base: v.base,
            max_size: v.max_size,
        }
    }
}
