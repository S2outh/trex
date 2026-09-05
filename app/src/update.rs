
use core::net::SocketAddr;

use embassy_boot_stm32::{AlignedBuffer, FirmwareUpdater, FirmwareUpdaterConfig, State};
use embassy_net::tcp::{self, TcpSocket};
use embassy_stm32::{flash::{Flash, Async, WRITE_SIZE}};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};

type Partition<'a> = embassy_embedded_hal::flash::partition::Partition<'a, NoopRawMutex, embassy_stm32::flash::Flash<'a>>;
type MtxFlash<'a> = Mutex<NoopRawMutex, Flash<'a, Async>>;

type Hash = [u8; blake3::OUT_LEN];

// In case of unexpected states this manager does not propagate errors,
// but instead reset the application.
macro_rules! reset {
    () => {
        cortex_m::peripheral::SCB::sys_reset()
    };
}

const CHUNK_SIZE: usize = 4096;
const MAGIC: [u8; 8] = *b"LEMMINGE";
const VALIDATE_HEADER_ID: u8 = 0;
const CHUNK_HEADER_ID: u8 = 1;
const APPLY_HEADER_ID: u8 = 2;


enum Header {
    Validate,
    Chunk {
        offset: usize,
        size: usize,
    },
    Apply {
        size: usize,
        hash: Hash,
    },
    Invalid,
}

pub struct FirmwareManagerStorage<'a> {
    flash: MtxFlash<'a>,
    magic: AlignedBuffer<WRITE_SIZE>,
}

impl<'a> FirmwareManagerStorage<'a> {
    pub async fn new(flash: Flash<'a, Async>) -> Self {
        let flash = Mutex::new(flash);
        let magic = AlignedBuffer([0; WRITE_SIZE]);
        Self { flash, magic }
    }
}

pub struct FirmwareManager<'a> {
    updater: FirmwareUpdater<'a, Partition<'a>, Partition<'a>>,
    socket: TcpSocket<'a>,
    address: SocketAddr,
    validated: bool,
}

impl<'a> FirmwareManager<'a> {
    pub async fn new(storage: &'a mut FirmwareManagerStorage<'a>, socket: TcpSocket<'a>, address: SocketAddr) -> Self {
        let config = FirmwareUpdaterConfig::from_linkerfile(&storage.flash, &storage.flash);
        let mut updater = FirmwareUpdater::new(config, &mut storage.magic.0);

        let validated =
            if let State::Swap = updater.get_state().await.unwrap_or_else(|_| reset!()) {
                false
            } else {
                true
            };

        Self { updater, socket, address, validated }
    }
    async fn set_validated(&mut self) {
        if !self.validated {
            self.validated = true;
            self.updater.mark_booted().await.unwrap_or_else(|_| reset!())
        }
    }
    async fn read_buf(&mut self, buf: &mut [u8], size: usize) -> Result<(), tcp::Error> {
        let mut pos = 0;
        while pos < size {
            pos += self.socket.read(&mut buf[pos..size]).await?;
        }
        Ok(())
    }
    async fn read<const N: usize>(&mut self) -> Result<[u8; N], tcp::Error> {
        let mut buf = [0; _];
        self.read_buf(&mut buf, N).await?;
        Ok(buf)
    }
    async fn read_byte(&mut self) -> Result<u8, tcp::Error> {
        Ok(u8::from_le_bytes(self.read().await?))
    }
    async fn read_word(&mut self) -> Result<usize, tcp::Error> {
        Ok(usize::from_le_bytes(self.read().await?))
    }
    async fn read_header(&mut self) -> Result<Header, tcp::Error> {
        match self.read_byte().await? {
            VALIDATE_HEADER_ID => Ok(Header::Validate),
            CHUNK_HEADER_ID => {
                let offset = self.read_word().await?;
                let size = self.read_word().await?;

                if size > CHUNK_SIZE {
                    Ok(Header::Invalid)
                } else {
                    Ok(Header::Chunk { offset, size })
                }
            },
            APPLY_HEADER_ID => {
                let size = self.read_word().await?;
                let hash = self.read().await?;
                Ok(Header::Apply { size, hash })
            },
            _ => Ok(Header::Invalid)
        }
    }
    async fn sync(&mut self) -> Result<Header, tcp::Error>  {
        let mut magic_pos = 0;
        loop {
            let mut byte = 0;
            self.socket.read(core::slice::from_mut(&mut byte)).await?;
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
    async fn run_connected(&mut self) {
        loop {
            let Ok(header) = self.sync().await else {
                return;
            };
            match header {
                Header::Validate => self.set_validated().await,
                Header::Chunk { offset, size } => {
                    if !self.validated {
                        continue;
                    }

                    let mut chunk = AlignedBuffer([0; CHUNK_SIZE]);
                    if let Err(_) = self.read_buf(chunk.as_mut(), size).await {
                        return;
                    };
                    if let Err(_) = self.updater.write_firmware(offset, chunk.as_ref()).await {
                        reset!()
                    }
                },
                Header::Apply { size, hash } => {
                    if !self.validated {
                        continue;
                    }
                    
                    let mut local_hash: Hash = [0u8; _];
                    let mut chunk_buf = [0u8; blake3::CHUNK_LEN];
                    if let Err(_) = self.updater.hash::<blake3::Hasher>(size as u32, &mut chunk_buf, &mut local_hash).await {
                        reset!();
                    }

                    if local_hash != hash {
                        continue;
                    }
                    
                    self.updater.mark_updated().await.unwrap_or_else(|_| reset!());
                    reset!()
                },
                Header::Invalid => (),
            }
        }
    }
    pub async fn run(&mut self) -> ! {
        loop {
            if let Err(_) = self.socket.accept(self.address).await {
                continue;
            }
            self.run_connected().await;
        }
    }
}

