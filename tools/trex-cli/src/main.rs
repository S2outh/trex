use anyhow::Result;

use clap::Parser;

use south_common::chell::ChellDefinition;
use south_common::types::trex::Command;
use south_common::definitions::groundstation::trex as defs;

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[arg(long)]
    pub az: Option<f32>,
    #[arg(long)]
    pub el: Option<f32>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let nats_client =
        async_nats::ConnectOptions::with_user_and_password("nats".into(), "south".into())
            .connect("nats.lan")
            .await?;

    println!("connected");

    if let Some(az) = cli.az {
        let command = Command::RotateAz(az);

        nats_client
            .publish(
                defs::Command.address(),
                minicbor_serde::to_vec(&command)?.into(),
            )
            .await?;
    }

    if let Some(el) = cli.el {
        let command = Command::RotateEl(el);

        nats_client
            .publish(
                defs::Command.address(),
                minicbor_serde::to_vec(&command)?.into(),
            )
            .await?;
    }

    nats_client.flush().await?;

    println!("command sent");

    Ok(())
}
