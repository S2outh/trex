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
    F: AsyncFnMut(&mut [u8], usize) -> Result<(), E>
{
    read_buf: F
}

impl<F, E> HeaderDeserializer<F, E>
where
    F: AsyncFnMut(&mut [u8], usize) -> Result<(), E>
{
    pub fn new(read_buf: F) -> Self {
        Self { read_buf }
    }

    async fn read<const N: usize>(&mut self) -> Result<[u8; N], E> {
        let mut buf = [0; _];
        (self.read_buf)(&mut buf, N).await?;
        Ok(buf)
    }

    async fn read_byte(&mut self) -> Result<u8, E> {
        Ok(u8::from_le_bytes(self.read().await?))
    }

    async fn read_word(&mut self) -> Result<usize, E> {
        Ok(usize::from_le_bytes(self.read().await?))
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
            HeaderType::Invalid(t) => Header::Invalid(t)
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
