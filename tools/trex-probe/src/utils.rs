use std::time::Duration;

use anyhow::{Context, Result, bail};
use console::StyledObject;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::{io::AsyncReadExt, net::TcpStream};
use trex_firmware_transport::{Deserializer, Response};

use crate::NetConf;

pub async fn connect_loading<D: std::fmt::Display>(
    net_conf: &NetConf,
    prefix: &StyledObject<D>,
) -> Result<TcpStream> {
    const SP_TEMPLATE: &str = "{spinner} Connecting to host...";
    let progress_style =
        ProgressStyle::with_template(&format!("{} {}", prefix, SP_TEMPLATE)).unwrap();
    let spinner = ProgressBar::new_spinner().with_style(progress_style);
    spinner.enable_steady_tick(Duration::from_millis(100));
    let tcp = TcpStream::connect((net_conf.host.clone(), net_conf.firmware_port))
        .await
        .context("could not connect to target")?;
    spinner.finish_and_clear();

    Ok(tcp)
}

pub async fn wait_for_host<D: std::fmt::Display>(net_conf: &NetConf, prefix: &StyledObject<D>) {
    const SP_TEMPLATE: &str = "{spinner} Waiting for host to reboot...";
    let progress_style =
        ProgressStyle::with_template(&format!("{} {}", prefix, SP_TEMPLATE)).unwrap();
    let spinner = ProgressBar::new_spinner().with_style(progress_style);
    spinner.enable_steady_tick(Duration::from_millis(100));

    while TcpStream::connect((net_conf.host.clone(), net_conf.firmware_port))
        .await
        .is_err()
    {}

    spinner.finish_and_clear();
}

pub async fn await_response(tcp: &mut TcpStream) -> Result<()> {
    let read_all = async |a: &mut [u8]| -> std::result::Result<(), tokio::io::Error> {
        tcp.read_exact(a).await?;
        Ok(())
    };

    let response = Deserializer::new(read_all)
        .read_response()
        .await
        .context("could not receive response")?;

    match response {
        Response::Success => Ok(()),
        Response::Fail => bail!("Target was unable to perform operation"),
        Response::Invalid(b) => bail!("Invalid response: {}", b),
    }
}
