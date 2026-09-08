
use anyhow::{Context, Result, bail};
use object::{Endianness, Object, ObjectSegment, read::elf::{ElfFile32, ProgramHeader}};

use crate::FlashConf;

pub fn elf_objectcopy(data: &[u8]) -> Result<(u64, Vec<u8>)> {
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

pub fn validate_object(base: u64, size: usize, flash_conf: &FlashConf) -> Result<()> {
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
