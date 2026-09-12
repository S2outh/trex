use std::{fs, path::PathBuf};

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use trex_firmware_transport::*;

use anyhow::{Context, Result};
use console::style;

#[macro_use]
mod helpers {
    macro_rules! vprintln {
        ($verbose: expr, $($r:tt)*) => {
            if $verbose {
                println!($($r)*);
            }
        };
    }
}

mod defmt_logger;
mod flash;

pub struct NetConf {
    pub host: String,
    pub firmware_port: u16,
    pub logger_port: u16,
}

pub struct FlashConf {
    pub path: PathBuf,
    pub base: Option<u64>,
    pub max_size: Option<u64>,
}

pub async fn run(net_conf: &NetConf, flash_conf: &FlashConf, v: bool) -> Result<()> {
    vprintln!(v, "{} Loading image", style("[RUN]").yellow());

    let data = fs::read(flash_conf.path.clone()).context("Failed to load image")?;

    vprintln!(v, "{} Successfully loaded image", style("[RUN]").yellow());

    flash::flash_elf(data.as_ref(), net_conf, flash_conf, v)
        .await
        .context("Failed to flash")?;

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    validate(net_conf, v).await.context("Validation failed")?;

    vprintln!(v, "{} Launching defmt logger...", style("[RUN]").yellow());
    vprintln!(v, "{}", style("----------------------------------").yellow());

    defmt_logger::run(data.as_ref(), net_conf)
        .await
        .context("defmt logger failed")
}

pub async fn attach(net_conf: &NetConf, path: PathBuf, v: bool) -> Result<()> {
    vprintln!(v, "{} Loading image", style("[Attach]").yellow());

    let data = fs::read(path).context("Failed to load image")?;

    vprintln!(v, "{} Successfully loaded image", style("[Attach]").yellow());
    vprintln!(v, "{} Launching defmt logger...", style("[Attach]").yellow());
    vprintln!(v, "{}", style("----------------------------------").yellow());

    defmt_logger::run(data.as_ref(), net_conf)
        .await
        .context("defmt logger failed")
}

pub async fn flash(net_conf: &NetConf, flash_conf: &FlashConf, v: bool) -> Result<()> {
    vprintln!(v, "{} Loading image", style("[FLASH]").cyan());

    let data = fs::read(flash_conf.path.clone()).context("Failed to load image")?;

    vprintln!(v, "{} Successfully loaded image", style("[FLASH]").cyan());

    flash::flash_elf(data.as_ref(), net_conf, flash_conf, v).await
}

pub async fn validate(net_conf: &NetConf, v: bool) -> Result<()> {
    vprintln!(v, "{} Connecting to target...", style("[VALIDATE]").green());

    let mut tcp = TcpStream::connect((net_conf.host.clone(), net_conf.firmware_port))
        .await
        .context("could not connect to target")?;

    vprintln!(v, 
        "{} Sending validation request...",
        style("[VALIDATE]").green()
    );

    HeaderSerializer::new(async |a| tcp.write_all(a).await)
        .write_header(Header::Validate)
        .await
        .context("could not send header")?;

    vprintln!(v, "{} Done!", style("[VALIDATE]").green());

    Ok(())
}

pub async fn reset(net_conf: &NetConf, v: bool) -> Result<()> {
    vprintln!(v, "{} Connecting to target...", style("[RESET]").red());

    let mut tcp = TcpStream::connect((net_conf.host.clone(), net_conf.firmware_port))
        .await
        .context("could not connect to target")?;

    vprintln!(v, "{} Sending reset request...", style("[RESET]").red());

    HeaderSerializer::new(async |a| tcp.write_all(a).await)
        .write_header(Header::Reset)
        .await
        .context("could not send header")?;

    vprintln!(v, "{} Done!", style("[RESET]").red());

    Ok(())
}
