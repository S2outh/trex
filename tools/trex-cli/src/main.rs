use anyhow::Result;

use tokio_stream::StreamExt;

use clap::{Args, Parser, Subcommand};

use nalgebra as na;

use south_common::chell::{ChellDefinition, match_def};
use south_common::definitions::groundstation::{self, trex as defs};
use south_common::types::trex::{Command, State};

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

// This needs to be done via chell in the long run,
// but rn chell ground support for deser is nonexistent :(
#[derive(serde::Deserialize, Debug)]
struct TMValue<T> {
    #[allow(unused)]
    timestamp: u64,
    #[allow(unused)]
    value: T,
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
            let mut sub = nats_client
                .subscribe(format!("{}.>", defs::base_address()))
                .await
                .unwrap();
            loop {
                let msg = sub.next().await.unwrap();
                let def = groundstation::from_address(msg.subject.as_str()).unwrap();
                match_def!(def, {
                    defs::State => {
                        let v = minicbor_serde::from_slice::<TMValue<State>>(&msg.payload)?;
                        println!("[State] {:#?}", v);
                    },
                    defs::Angles => {
                        let v = minicbor_serde::from_slice::<TMValue<na::Vector2<f64>>>(&msg.payload)?;
                        println!("[Angles] {:#?}", v);
                    },
                    defs::AngularVelocities => {
                        let v = minicbor_serde::from_slice::<TMValue<na::Vector2<f64>>>(&msg.payload)?;
                        println!("[Angular Velocities] {:#?}", v);
                    },
                });
            }
        }
    }

    nats_client.flush().await?;

    println!("command sent");

    Ok(())
}
