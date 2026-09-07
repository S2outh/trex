
use std::path::PathBuf;
use clap::{Args, Parser, Subcommand};

use trex_probe::{FlashConf, NetConf};

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, short, default_value = "192.168.0.10")]
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
    pub base: Option<u64>,
    /// length of active partition to optionally check against
    pub size: Option<u64>,
}

impl From<&Cli> for NetConf {
    fn from(v: &Cli) -> Self {
        NetConf { host: v.host.clone(), port: v.port }
    }
}

impl From<FlashArgs> for FlashConf {
    fn from(v: FlashArgs) -> Self {
        FlashConf { path: v.path, base: v.base, size: v.size }
    }
}
