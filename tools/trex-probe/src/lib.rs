use std::{fs, path::PathBuf};

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use trex_firmware_transport::*;

use anyhow::{Context, Result};
use console::style;
use indicatif::{ProgressIterator, ProgressStyle};

mod flash;

pub struct NetConf {
    pub host: String,
    pub port: u16,
}

pub struct FlashConf {
    pub path: PathBuf,
    pub base: Option<u64>,
    pub max_size: Option<u64>,
}

const PR_TEMPLATE: &str = "{spinner} {bar:60.green/blue} Sending Chunk: {pos}/{len} [{elapsed}]";
const PR_CHARS: &str = "##-";

pub async fn run(net_conf: &NetConf, flash_conf: &FlashConf) -> Result<()> {
    flash(net_conf, flash_conf)
        .await
        .context("Failed to flash")?;

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    validate(net_conf).await.context("Validation failed")?;

    Ok(())
}

pub async fn flash(net_conf: &NetConf, flash_conf: &FlashConf) -> Result<()> {
    println!("{} Loading image", style("[FLASH]").cyan());

    let data = fs::read(flash_conf.path.clone()).context("Failed to load image")?;
    let (base, object) = flash::elf_objectcopy(data.as_ref()).context("Failed to load image")?;

    let size = object.len();
    flash::validate_object(base, size, flash_conf).context("ELF validation failed")?;

    println!("{} Successfully loaded image", style("[FLASH]").cyan());

    let hash = blake3::hash(&object).into();

    println!("{} Connecting to target...", style("[FLASH]").cyan());

    let mut tcp = TcpStream::connect((net_conf.host.clone(), net_conf.port))
        .await
        .context("could not connect to target")?;

    println!("{} Sending firmware...", style("[FLASH]").cyan());

    let progress_style = ProgressStyle::with_template(PR_TEMPLATE)
        .unwrap()
        .progress_chars(PR_CHARS);
    for (i, chunk) in object
        .chunks(CHUNK_SIZE)
        .enumerate()
        .progress_with_style(progress_style)
    {
        let offset = i * CHUNK_SIZE;
        let size = chunk.len();
        HeaderSerializer::new(async |a| tcp.write_all(a).await)
            .write_header(Header::Chunk { offset, size })
            .await
            .context("could not send header")?;

        tcp.write_all(chunk).await.context("could not send chunk")?;
    }

    println!("{} Applying firmware...", style("[FLASH]").cyan());

    HeaderSerializer::new(async |a| tcp.write_all(a).await)
        .write_header(Header::Apply { size, hash })
        .await
        .context("could not send header")?;

    println!("{} Done!", style("[FLASH]").cyan());

    Ok(())
}

pub async fn validate(net_conf: &NetConf) -> Result<()> {
    println!("{} Connecting to target...", style("[VALIDATE]").green());

    let mut tcp = TcpStream::connect((net_conf.host.clone(), net_conf.port))
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

    let mut tcp = TcpStream::connect((net_conf.host.clone(), net_conf.port))
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
