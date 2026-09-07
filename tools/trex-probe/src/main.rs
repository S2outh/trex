
use anyhow::Result;

use clap::Parser;

use trex_probe;

mod cli;
use cli::{Cli, Commands};

fn main() -> Result<()> {
    let cli = Cli::parse();

    let net_conf = (&cli).into();
    match cli.command {
        Commands::Run(args) => trex_probe::run(&net_conf, &args.into()),
        Commands::Flash(args) => trex_probe::flash(&net_conf, &args.into()),
        Commands::Validate => trex_probe::validate(&net_conf),
        Commands::Reset => trex_probe::reset(&net_conf),
    }
}
