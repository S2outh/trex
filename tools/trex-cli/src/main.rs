use anyhow::Result;

use clap::Parser;

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    pub v: f64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let nats_client =
        async_nats::ConnectOptions::with_user_and_password("nats".into(), "south".into())
            .connect("nats.lan")
            .await?;

    println!("connected");

    #[derive(serde::Serialize, Debug)]
    struct TestTarget {
        v: f64,
    }
    let target = TestTarget { v: cli.v };

    nats_client
        .publish(
            "trex.testing.target",
            minicbor_serde::to_vec(&target)?.into(),
        )
        .await?;

    nats_client.flush().await?;

    println!("sent: {:?}", target);

    Ok(())
}
