
use anyhow::{Context, Result, bail};
use object::{Endianness, Object, ObjectSegment, read::elf::{ElfFile32, ProgramHeader}};

use crate::FlashConf;

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
    let base = segments
        .iter()
        .map(|(lma, _)| *lma)
        .min()
        .unwrap();
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
            bail!("Binary base address missmatch: was {}, expected {}", base, conf_base)
        }
    }
    if let Some(conf_max_size) = flash_conf.max_size {
        if size > conf_max_size as usize {
            bail!("Binary size too large: was {}, expected max {}", size, conf_max_size)
        }
    }
    Ok(())
}
