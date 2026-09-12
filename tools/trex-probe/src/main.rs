use anyhow::Result;

use clap::Parser;

use trex_probe;

mod cli;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let v = cli.verbose;
    let net_conf = (&cli).into();
    match cli.command {
        Commands::Run(args) => trex_probe::run(&net_conf, &args.into(), v).await,
        Commands::Flash(args) => trex_probe::flash(&net_conf, &args.into(), v).await,
        Commands::Attach(args) => trex_probe::attach(&net_conf, args.path, v).await,
        Commands::Validate => trex_probe::validate(&net_conf, v).await,
        Commands::Reset => trex_probe::reset(&net_conf, v).await,
    }
}
