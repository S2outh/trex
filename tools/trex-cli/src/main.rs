use anyhow::Result;

use tokio_stream::StreamExt;

use clap::{Args, Parser, Subcommand};

use nalgebra as na;

use south_common::chell::ChellDefinition;
use south_common::definitions::groundstation::trex as defs;
use south_common::types::trex::Command;

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, short, default_value = "nats.lan")]
    pub nats_host: String,

    #[arg(long, short, default_value = "nats")]
    pub nats_user: String,

    #[arg(long, short, default_value = "south")]
    pub nats_pwd: String,
}

#[derive(Subcommand)]
pub enum Commands {
    SetTarget(TargetArgs),
    ReadTm,
}

#[derive(Args)]
pub struct TargetArgs {
    #[arg(long, default_value_t = 0.)]
    pub az: f64,
    #[arg(long, default_value_t = 0.)]
    pub el: f64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let nats_client =
        async_nats::ConnectOptions::with_user_and_password(cli.nats_user, cli.nats_pwd)
            .connect(cli.nats_host)
            .await?;

    println!("connected");

    match cli.command {
        Commands::SetTarget(args) => {
            let command = Command::Rotate(na::Vector2::new(args.az, args.el));

            nats_client
                .publish(
                    defs::Command.address(),
                    minicbor_serde::to_vec(&command)?.into(),
                )
                .await?;
        }
        Commands::ReadTm => {
            let mut sub = nats_client.subscribe(format!("{}.>", defs::base_address())).await.unwrap();
            loop {
                let next = sub.next().await.unwrap();
                println!("{:?}", next.payload);
            }
        }
    }

    nats_client.flush().await?;

    println!("command sent");

    Ok(())
}
