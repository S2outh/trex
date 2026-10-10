use std::{fs, path::PathBuf};

use tokio::io::AsyncWriteExt;

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

mod flash;
mod logger;
mod utils;

pub struct NetConf {
    pub host: String,
    pub firmware_port: u16,
    pub logger_port: u16,
}

pub struct FlashConf {
    pub path: PathBuf,
    pub key: PathBuf,
    pub base: Option<u64>,
    pub max_size: Option<u64>,
}

pub async fn run(net_conf: &NetConf, flash_conf: &FlashConf, v: bool) -> Result<()> {
    vprintln!(v, "{} Loading image", style("[RUN]").yellow());

    let data = fs::read(flash_conf.path.clone()).context("Failed to load image")?;
    let key = fs::read(flash_conf.key.clone()).context("Failed to load key")?;

    vprintln!(v, "{} Successfully loaded image", style("[RUN]").yellow());

    flash::flash_elf(data.as_slice(), key.as_slice().try_into()?, net_conf, flash_conf, v)
        .await
        .context("Failed to flash")?;

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    validate(net_conf, v).await.context("Validation failed")?;

    vprintln!(v, "{} Launching defmt logger...", style("[RUN]").yellow());
    vprintln!(
        v,
        "{}",
        style("----------------------------------").yellow()
    );

    logger::run(data.as_ref(), net_conf)
        .await
        .context("defmt logger failed")
}

pub async fn attach(net_conf: &NetConf, path: PathBuf, v: bool) -> Result<()> {
    vprintln!(v, "{} Loading image", style("[Attach]").yellow());

    let data = fs::read(path).context("Failed to load image")?;

    vprintln!(
        v,
        "{} Successfully loaded image",
        style("[Attach]").yellow()
    );
    vprintln!(
        v,
        "{} Launching defmt logger...",
        style("[Attach]").yellow()
    );
    vprintln!(
        v,
        "{}",
        style("----------------------------------").yellow()
    );

    logger::run(data.as_ref(), net_conf)
        .await
        .context("defmt logger failed")
}

pub async fn flash(net_conf: &NetConf, flash_conf: &FlashConf, v: bool) -> Result<()> {
    vprintln!(v, "{} Loading image", style("[FLASH]").cyan());

    let data = fs::read(flash_conf.path.clone()).context("Failed to load image")?;
    let key = fs::read(flash_conf.key.clone()).context("Failed to load key")?;

    vprintln!(v, "{} Successfully loaded image", style("[FLASH]").cyan());

    flash::flash_elf(data.as_slice(), key.as_slice().try_into()?, net_conf, flash_conf, v).await
}

pub async fn size(path: PathBuf, v: bool) -> Result<()> {
    vprintln!(v, "{} Loading image", style("[SIZE]").blue());

    let data = fs::read(path).context("Failed to load image")?;

    vprintln!(v, "{} Successfully loaded image", style("[SIZE]").blue());

    flash::print_size(data.as_ref())
}

pub async fn validate(net_conf: &NetConf, v: bool) -> Result<()> {
    vprintln!(v, "{} Connecting to target...", style("[VALIDATE]").green());

    let mut tcp = utils::connect_loading(net_conf, style("[VALIDATE]").green()).await?;

    vprintln!(
        v,
        "{} Sending validation request...",
        style("[VALIDATE]").green()
    );

    Serializer::new(async |a| tcp.write_all(a).await)
        .write_header(Header::Validate)
        .await
        .context("could not send header")?;

    utils::await_response(&mut tcp).await.context("validation failed")?;

    vprintln!(v, "{} Done!", style("[VALIDATE]").green());

    Ok(())
}

pub async fn reset(net_conf: &NetConf, v: bool) -> Result<()> {
    vprintln!(v, "{} Connecting to target...", style("[RESET]").red());

    let mut tcp = utils::connect_loading(net_conf, style("[RESET]").red()).await?;

    vprintln!(v, "{} Sending reset request...", style("[RESET]").red());

    Serializer::new(async |a| tcp.write_all(a).await)
        .write_header(Header::Reset)
        .await
        .context("could not send header")?;

    utils::await_response(&mut tcp).await.context("reset failed")?;

    vprintln!(v, "{} Done!", style("[RESET]").red());

    Ok(())
}

pub async fn keygen(private_file: PathBuf, public_file: PathBuf, v: bool) -> Result<()> {
    vprintln!(v, "{} Generating keypair...", style("[KEYGEN]").magenta());

    let mut private_key = [0u8; 32];
    getrandom::fill(&mut private_key)?;
    let keypair = salty::Keypair::from(&private_key);
    let public_key = keypair.public.to_bytes();

    vprintln!(v, "{} Saving keypair...", style("[KEYGEN]").magenta());

    fs::write(private_file, private_key).context("could not create private key file")?;
    fs::write(public_file, public_key).context("could not create public key file")?;

    Ok(())
}
