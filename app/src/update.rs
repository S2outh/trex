
use embassy_boot_stm32::{AlignedBuffer, FirmwareUpdater, FirmwareUpdaterConfig, State};
use embassy_futures::yield_now;
use embassy_net::tcp::{self, TcpSocket};
use embassy_stm32::{flash::{Flash, Async, WRITE_SIZE}};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};

type Partition<'a> = embassy_embedded_hal::flash::partition::Partition<'a, NoopRawMutex, embassy_stm32::flash::Flash<'a>>;
type MtxFlash<'a> = Mutex<NoopRawMutex, Flash<'a, Async>>;

type Hash = [u8; blake3::OUT_LEN];

// In case of unexpected states this manager does not propagate errors,
// but instead reset the application.
macro_rules! reset {
    () => {{
        defmt::warn!("[FW MGR] Reset");
        cortex_m::peripheral::SCB::sys_reset();
    }};
}

const CHUNK_SIZE: usize = 4096;
const MAGIC: [u8; 8] = *b"LEMMINGE";
const VALIDATE_HEADER_ID: u8 = 0;
const CHUNK_HEADER_ID: u8 = 1;
const APPLY_HEADER_ID: u8 = 2;
const RESET_HEADER_ID: u8 = 3;


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
    Reset,
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
    port: u16,
    validated: bool,
}

impl<'a> FirmwareManager<'a> {
    pub async fn new(storage: &'a mut FirmwareManagerStorage<'a>, socket: TcpSocket<'a>, port: u16) -> Self {
        let config = FirmwareUpdaterConfig::from_linkerfile(&storage.flash, &storage.flash);
        let mut updater = FirmwareUpdater::new(config, &mut storage.magic.0);

        let validated =
            if let State::Swap = updater.get_state().await.unwrap_or_else(|_| reset!()) {
                false
            } else {
                true
            };

        defmt::info!("[FW MGR] Initializing firmware updater. validated: {}", validated);

        Self { updater, socket, port, validated }
    }
    async fn set_validated(&mut self) {
        if !self.validated {
            defmt::info!("[FW MGR] Firmware validated");
            self.validated = true;
            self.updater.mark_booted().await.unwrap_or_else(|_| reset!())
        }
    }
    async fn read_buf(&mut self, buf: &mut [u8], size: usize) -> Result<(), tcp::Error> {
        let mut pos = 0;
        while pos < size {
            let bytes_read = self.socket.read(&mut buf[pos..size]).await?;
            if bytes_read == 0 {
                return Err(tcp::Error::ConnectionReset)
            }
            pos += bytes_read;
            // Yield here to not block incase of large blocks of data
            yield_now().await;
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
            RESET_HEADER_ID => Ok(Header::Reset),
            _ => Ok(Header::Invalid)
        }
    }
    async fn sync(&mut self) -> Result<Header, tcp::Error>  {
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
    async fn run_connected(&mut self) {
        loop {
            let Ok(header) = self.sync().await else {
                defmt::error!("[FW MGR] Disconnected");
                return;
            };
            match header {
                Header::Validate => self.set_validated().await,
                Header::Chunk { offset, size } => {
                    defmt::info!("[FW MGR] Received chunk, offset: {}, size: {}", offset, size);

                    let mut chunk = AlignedBuffer([0; CHUNK_SIZE]);
                    if let Err(e) = self.read_buf(chunk.as_mut(), size).await {
                        defmt::error!("[FW MGR] Disconnected: {}", e);
                        return;
                    };

                    if !self.validated {
                        continue;
                    }

                    if let Err(e) = self.updater.write_firmware(offset, chunk.as_ref()).await {
                        defmt::error!("[FW MGR] Could not write firmware: {}", e);
                    }
                },
                Header::Apply { size, hash } => {
                    if !self.validated {
                        continue;
                    }

                    defmt::info!("[FW MGR] Trying to apply new firmware...");
                    
                    let mut local_hash: Hash = [0u8; _];
                    let mut chunk_buf = [0u8; blake3::CHUNK_LEN];
                    if let Err(e) = self.updater.hash::<blake3::Hasher>(size as u32, &mut chunk_buf, &mut local_hash).await {
                        defmt::error!("[FW MGR] Could not hash firmware: {}", e);
                        continue;
                    }

                    if local_hash != hash {
                        defmt::info!("[FW MGR] Invalid Hash");
                        continue;
                    }
                    defmt::info!("[FW MGR] Switching to new firmware...");
                    
                    let _ = self.updater.mark_updated().await;
                    reset!()
                },
                Header::Reset => reset!(),
                Header::Invalid => defmt::warn!("[FW MGR] Received invalid Header!"),
            }
        }
    }
    async fn disconnect(&mut self) {
        self.socket.abort();
        let _ = self.socket.flush().await;
    }
    pub async fn run(&mut self) -> ! {
        loop {
            defmt::info!("[FW MGR] Waiting for connections...");
            if let Err(_) = self.socket.accept(self.port).await {
                self.disconnect().await;
                continue;
            }
            defmt::info!("[FW MGR] Connection established");
            self.run_connected().await;
            defmt::info!("[FW MGR] Connection reset");
            self.disconnect().await;
        }
    }
}

