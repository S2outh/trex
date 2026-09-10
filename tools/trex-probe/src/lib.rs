use std::{fs, path::PathBuf};

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use trex_firmware_transport::*;

use anyhow::{Context, Result};
use console::style;

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

pub async fn run(net_conf: &NetConf, flash_conf: &FlashConf) -> Result<()> {
    println!("{} Loading image", style("[RUN]").yellow());

    let data = fs::read(flash_conf.path.clone()).context("Failed to load image")?;

    println!("{} Successfully loaded image", style("[RUN]").yellow());

    flash::flash_elf(data.as_ref(), net_conf, flash_conf)
        .await
        .context("Failed to flash")?;

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    validate(net_conf).await.context("Validation failed")?;

    defmt_logger::run(data.as_ref(), net_conf)
        .await
        .context("defmt logger failed")
}

pub async fn attach(net_conf: &NetConf, path: PathBuf) -> Result<()> {
    println!("{} Loading image", style("[Attach]").yellow());

    let data = fs::read(path).context("Failed to load image")?;

    println!("{} Successfully loaded image", style("[Attach]").yellow());

    defmt_logger::run(data.as_ref(), net_conf)
        .await
        .context("defmt logger failed")
}

pub async fn flash(net_conf: &NetConf, flash_conf: &FlashConf) -> Result<()> {
    println!("{} Loading image", style("[FLASH]").cyan());

    let data = fs::read(flash_conf.path.clone()).context("Failed to load image")?;

    println!("{} Successfully loaded image", style("[FLASH]").cyan());

    flash::flash_elf(data.as_ref(), net_conf, flash_conf).await
}

pub async fn validate(net_conf: &NetConf) -> Result<()> {
    println!("{} Connecting to target...", style("[VALIDATE]").green());

    let mut tcp = TcpStream::connect((net_conf.host.clone(), net_conf.firmware_port))
        .await
        .context("could not connect to target")?;

    println!(
        "{} Sending validation request...",
        style("[VALIDATE]").green()
    );

    HeaderSerializer::new(async |a| tcp.write_all(a).await)
        .write_header(Header::Validate)
        .await
        .context("could not send header")?;

    println!("{} Done!", style("[VALIDATE]").green());

    Ok(())
}

pub async fn reset(net_conf: &NetConf) -> Result<()> {
    println!("{} Connecting to target...", style("[RESET]").red());

    let mut tcp = TcpStream::connect((net_conf.host.clone(), net_conf.firmware_port))
        .await
        .context("could not connect to target")?;

    println!("{} Sending reset request...", style("[RESET]").red());

    HeaderSerializer::new(async |a| tcp.write_all(a).await)
        .write_header(Header::Reset)
        .await
        .context("could not send header")?;

    println!("{} Done!", style("[RESET]").red());

    Ok(())
}
