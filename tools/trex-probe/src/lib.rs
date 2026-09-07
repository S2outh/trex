use std::io::Write;
use std::{fs, path::PathBuf};
use std::net::{TcpStream};

use trex_transport::*;

use console::style;
use indicatif::{ProgressIterator};
use anyhow::{Context, Result, bail};
use object::{Endianness, Object, ObjectSegment, read::elf::{ElfFile32, ProgramHeader}};

pub struct NetConf {
    pub host: String,
    pub port: u16,
}

pub struct FlashConf {
    pub path: PathBuf,
    pub base: Option<u64>,
    pub size: Option<u64>,
}

pub fn run(net_conf: &NetConf, flash_conf: &FlashConf) -> Result<()> {

    println!("{} Flashing...", style("[RUN]").yellow());
    flash(net_conf, flash_conf).context("Failed to flash")?;

    // TEMP TODO
    std::thread::sleep(std::time::Duration::from_secs(5));

    println!("{} Validating...", style("[RUN]").yellow());
    validate(net_conf).context("Validation failed")?;

    Ok(())
}

fn elf_objectcopy(data: &[u8]) -> Result<(u64, Vec<u8>)> {
    let file = ElfFile32::<Endianness>::parse(data)?;
    let endian = file.endian();

    let mut chunks = file
        .segments()
        .map(|seg| {
            let lma: u64 = seg.elf_program_header().p_paddr(endian).into();
            seg.data().map(|data| (lma, data))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|(_, data)| !data.is_empty())
        .collect::<Vec<_>>();

    if chunks.is_empty() {
        bail!("no loadable segments in image")
    }

    chunks.sort_by_key(|(lma, _)| *lma);

    let base = chunks.first().unwrap().0;
    let end = chunks
        .iter()
        .map(|(lma, data)| lma + data.len() as u64)
        .max()
        .unwrap();

    let len = usize::try_from(end - base).context("image too large")?;
    let mut out = vec![0u8; len];

    for (lma, data) in chunks {
        let offset = (lma - base) as usize;
        out[offset..(offset + data.len())].copy_from_slice(data);
    }

    Ok((base, out))
}

fn validate_object(base: u64, size: usize, flash_conf: &FlashConf) -> Result<()> {
    if let Some(conf_base) = flash_conf.base {
        if base != conf_base {
            bail!("Binary base address missmatch")
        }
    }
    if let Some(conf_size) = flash_conf.size {
        if size != conf_size as usize {
            bail!("Binary length missmatch")
        }
    }
    Ok(())
}

pub fn flash(net_conf: &NetConf, flash_conf: &FlashConf) -> Result<()> {

    println!("{} Loading image", style("[FLASH]").cyan());

    let data = fs::read(flash_conf.path.clone()).context("Failed to load image")?;
    let (base, object) = elf_objectcopy(data.as_ref()).context("Failed to load image")?;

    let size = object.len();
    validate_object(base, size, flash_conf).context("ELF validation failed")?;
    
    println!("{} Successfully loaded image", style("[FLASH]").cyan());

    let hash = blake3::hash(&object).into();
    
    println!("{} Connecting to target...", style("[FLASH]").cyan());

    let mut tcp = TcpStream::connect((net_conf.host.clone(), net_conf.port)).context("could not connect to target")?;

    println!("{} Sending firmware...", style("[FLASH]").cyan());

    for (i, chunk) in object.chunks(CHUNK_SIZE).enumerate().progress() {
        let offset = i * CHUNK_SIZE;
        let size = chunk.len();
        HeaderSerializer::new(|a| tcp.write_all(a))
            .write_header(Header::Chunk { offset, size }).context("could not send header")?;

        tcp.write_all(chunk).context("could not send chunk")?;
    }

    println!("{} Applying firmware...", style("[FLASH]").cyan());

    HeaderSerializer::new(|a| tcp.write_all(a))
        .write_header(Header::Apply { size, hash }).context("could not send header")?;

    println!("{} Done!", style("[FLASH]").cyan());
    
    Ok(())
}

pub fn validate(net_conf: &NetConf) -> Result<()> {

    println!("{} Connecting to target...", style("[VALIDATE]").green());

    let mut tcp = TcpStream::connect((net_conf.host.clone(), net_conf.port)).context("could not connect to target")?;

    println!("{} Sending validation request...", style("[VALIDATE]").green());

    HeaderSerializer::new(|a| tcp.write_all(a))
        .write_header(Header::Validate).context("could not send header")?;
    
    println!("{} Done!", style("[VALIDATE]").green());

    Ok(())
}

pub fn reset(net_conf: &NetConf) -> Result<()> {

    println!("{} Connecting to target...", style("[RESET]").red());

    let mut tcp = TcpStream::connect((net_conf.host.clone(), net_conf.port)).context("could not connect to target")?;

    println!("{} Sending reset request...", style("[RESET]").red());

    HeaderSerializer::new(|a| tcp.write_all(a))
        .write_header(Header::Reset).context("could not send header")?;

    println!("{} Done!", style("[RESET]").red());

    Ok(())
}
