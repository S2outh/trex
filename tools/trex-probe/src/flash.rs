use trex_firmware_transport::*;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

use anyhow::{Context, Result, bail};
use console::style;
use object::{
    Endianness, Object, ObjectSegment,
    read::elf::{ElfFile32, ProgramHeader},
};

use indicatif::{ProgressIterator, ProgressStyle};

use crate::{FlashConf, NetConf};

const PR_TEMPLATE: &str = "Sending: [{bar:30.green/blue}] Chunk: {pos}/{len} [{elapsed}]";
const PR_CHARS: &str = "=>-";

pub fn elf_objectcopy(data: &[u8]) -> Result<(u64, Vec<u8>)> {
    let file = ElfFile32::<Endianness>::parse(data)?;
    let endian = file.endian();

    let segments = file
        .segments()
        .map(|seg| {
            // Get physical address from elf programm header. The physical address
            // is the address a segment should be stored at, while the (more common)
            // virtual address describes the location of the code at runtime. for embedded
            // systems these differ in some cases.
            let lma: u64 = seg.elf_program_header().p_paddr(endian).into();
            seg.data().map(|data| (lma, data))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|(_, data)| !data.is_empty())
        .collect::<Vec<_>>();

    if segments.is_empty() {
        bail!("no loadable segments in image")
    }

    // Base is the lowest physical address of any segment
    let base = segments.iter().map(|(lma, _)| *lma).min().unwrap();
    // End is the highest end address (lma + length) of any segment
    let end = segments
        .iter()
        .map(|(lma, data)| lma + data.len() as u64)
        .max()
        .unwrap();

    let len = usize::try_from(end - base).context("image too large")?;

    // pre allocate zeros, in order to guarantee zeroed data for empty sections in the binary
    let mut out = vec![0u8; len];

    // write the segments in the same order they appear in in the elf file.
    // If later segments overlap earlier segments the earlier ones will be overwritten.
    for (lma, data) in segments {
        let offset = (lma - base) as usize;
        out[offset..(offset + data.len())].copy_from_slice(data);
    }

    Ok((base, out))
}

pub fn validate_object(base: u64, size: usize, flash_conf: &FlashConf) -> Result<()> {
    if let Some(conf_base) = flash_conf.base {
        if base != conf_base {
            bail!(
                "Binary base address missmatch: was {}, expected {}",
                base,
                conf_base
            )
        }
    }
    if let Some(conf_max_size) = flash_conf.max_size {
        if size > conf_max_size as usize {
            bail!(
                "Binary size too large: was {}, expected max {}",
                size,
                conf_max_size
            )
        }
    }
    Ok(())
}

pub async fn flash_elf(elf: &[u8], net_conf: &NetConf, flash_conf: &FlashConf) -> Result<()> {
    let (base, object) = elf_objectcopy(elf).context("Failed to load image")?;

    let size = object.len();
    validate_object(base, size, flash_conf).context("ELF validation failed")?;

    println!("{} Successfully validated image", style("[FLASH]").cyan());

    let hash = blake3::hash(&object).into();

    println!("{} Connecting to target...", style("[FLASH]").cyan());

    let mut tcp = TcpStream::connect((net_conf.host.clone(), net_conf.firmware_port))
        .await
        .context("could not connect to target")?;

    println!("{} Sending firmware...", style("[FLASH]").cyan());

    let progress_style = ProgressStyle::with_template(&format!("{} {}", style("[FLASH]").cyan(), PR_TEMPLATE))
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
