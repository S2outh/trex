#![no_std]

use num_enum::{FromPrimitive, IntoPrimitive};

pub type Hash = [u8; blake3::OUT_LEN];

pub const MAGIC: [u8; 4] = *b"TREX";
pub const CHUNK_SIZE: usize = 4096;

#[derive(FromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum HeaderType {
    Validate,
    Chunk,
    Apply,
    Reset,
    #[num_enum(catch_all)]
    Invalid(u8),
}

pub enum Header {
    Validate,
    Chunk {
        offset: usize,
        size: usize,
    },
    Apply {
        size: usize,
        hash: Hash,
    },
    Reset,
    Invalid(u8),
}

pub struct HeaderDeserializer<F, E> 
where
    F: AsyncFnMut(&mut [u8]) -> Result<(), E>
{
    read_buf: F
}

impl<F, E> HeaderDeserializer<F, E>
where
    F: AsyncFnMut(&mut [u8]) -> Result<(), E>
{
    pub fn new(read_buf: F) -> Self {
        Self { read_buf }
    }

    async fn read<const N: usize>(&mut self) -> Result<[u8; N], E> {
        let mut buf = [0; _];
        (self.read_buf)(&mut buf).await?;
        Ok(buf)
    }

    async fn read_byte(&mut self) -> Result<u8, E> {
        Ok(u8::from_le_bytes(self.read().await?))
    }

    async fn read_word(&mut self) -> Result<usize, E> {
        Ok(u32::from_le_bytes(self.read().await?) as usize)
    }

    async fn read_header(&mut self) -> Result<Header, E> {
        let header_type = HeaderType::from(self.read_byte().await?);
        let header = match header_type {
            HeaderType::Validate => Header::Validate,
            HeaderType::Chunk => {
                let offset = self.read_word().await?;
                let size = self.read_word().await?;

                if size > CHUNK_SIZE {
                    Header::Invalid(0)
                } else {
                    Header::Chunk { offset, size }
                }
            },
            HeaderType::Apply => {
                let size = self.read_word().await?;
                let hash = self.read().await?;
                Header::Apply { size, hash }
            },
            HeaderType::Reset => Header::Reset,
            HeaderType::Invalid(t) => Header::Invalid(t),
        };
        Ok(header)
    }

    pub async fn sync(&mut self) -> Result<Header, E> {
        let mut magic_pos = 0;
        loop {
            let byte = self.read_byte().await?;
            if byte == MAGIC[magic_pos] {
                magic_pos += 1;
                if magic_pos == MAGIC.len() {
                    return self.read_header().await;
                }
            } else {
                magic_pos = 0;
            }
        }
    }
}

pub struct HeaderSerializer<F, E> 
where
    F: FnMut(&[u8]) -> Result<(), E>
{
    write_buf: F
}

impl<F, E> HeaderSerializer<F, E>
where
    F: FnMut(&[u8]) -> Result<(), E>
{
    pub fn new(write_buf: F) -> Self {
        Self { write_buf }
    }

    fn write_byte(&mut self, v: u8) -> Result<(), E> {
        Ok((self.write_buf)(&v.to_le_bytes())?)
    }

    fn write_word(&mut self, v: usize) -> Result<(), E> {
        Ok((self.write_buf)(&(v as u32).to_le_bytes())?)
    }

    pub fn write_header(&mut self, header: Header) -> Result<(), E> {
        (self.write_buf)(&MAGIC)?;

        match header {
            Header::Validate => {
                self.write_byte(HeaderType::Validate.into())?;
            },
            Header::Chunk { offset, size } => {
                self.write_byte(HeaderType::Chunk.into())?;
                self.write_word(offset)?;
                self.write_word(size)?;
            },
            Header::Apply { size, hash } => {
                self.write_byte(HeaderType::Apply.into())?;
                self.write_word(size)?;
                (self.write_buf)(&hash)?;
            },
            Header::Reset => {
                self.write_byte(HeaderType::Reset.into())?;
            },
            Header::Invalid(_) => (),
        }
        Ok(())
    }
}
