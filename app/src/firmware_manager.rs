use embassy_boot_stm32::{AlignedBuffer, FirmwareUpdater, FirmwareUpdaterConfig, State};
use embassy_futures::yield_now;
use embassy_net::tcp::{self, TcpSocket};
use embassy_stm32::flash::{Async, Flash, WRITE_SIZE};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use trex_firmware_transport::*;

type Partition<'a> = embassy_embedded_hal::flash::partition::Partition<
    'a,
    NoopRawMutex,
    embassy_stm32::flash::Flash<'a>,
>;
type MtxFlash<'a> = Mutex<NoopRawMutex, Flash<'a, Async>>;

// In case of unexpected states this manager does not propagate errors,
// but instead reset the application.
macro_rules! reset {
    () => {{
        defmt::warn!("[FW MGR] Reset");
        cortex_m::peripheral::SCB::sys_reset();
    }};
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
    pub async fn new(
        storage: &'a mut FirmwareManagerStorage<'a>,
        socket: TcpSocket<'a>,
        port: u16,
    ) -> Self {
        let config = FirmwareUpdaterConfig::from_linkerfile(&storage.flash, &storage.flash);
        let mut updater = FirmwareUpdater::new(config, &mut storage.magic.0);

        let validated = if let State::Swap = updater.get_state().await.unwrap_or_else(|_| reset!())
        {
            false
        } else {
            true
        };

        defmt::info!(
            "[FW MGR] Initializing firmware updater. validated: {}",
            validated
        );

        Self {
            updater,
            socket,
            port,
            validated,
        }
    }
    async fn set_validated(&mut self) {
        if !self.validated {
            defmt::info!("[FW MGR] Firmware validated");
            self.validated = true;
            self.updater
                .mark_booted()
                .await
                .unwrap_or_else(|_| reset!())
        }
    }
    async fn read_buf(&mut self, buf: &mut [u8]) -> Result<(), tcp::Error> {
        let mut pos = 0;
        while pos < buf.len() {
            let bytes_read = self.socket.read(&mut buf[pos..]).await?;
            if bytes_read == 0 {
                return Err(tcp::Error::ConnectionReset);
            }
            pos += bytes_read;
            // Yield here to not block in case of large blocks of data
            yield_now().await;
        }
        Ok(())
    }
    async fn run_connected(&mut self) {
        loop {
            let Ok(header) = HeaderDeserializer::new(async |a| self.read_buf(a).await)
                .sync()
                .await
            else {
                return;
            };
            match header {
                Header::Validate => self.set_validated().await,
                Header::Chunk { offset, size } => {
                    defmt::info!(
                        "[FW MGR] Received chunk, offset: {}, size: {}",
                        offset,
                        size
                    );

                    let mut chunk = AlignedBuffer([0; CHUNK_SIZE]);
                    if let Err(_) = self.read_buf(&mut chunk.as_mut()[..size]).await {
                        return;
                    };

                    if !self.validated {
                        defmt::warn!("[FW MGR] Not validated, ignoring incoming chunk");
                        continue;
                    }

                    if let Err(e) = self.updater.write_firmware(offset, chunk.as_ref()).await {
                        defmt::error!("[FW MGR] Could not write firmware: {}", e);
                    }
                }
                Header::Apply { size, hash } => {
                    if !self.validated {
                        defmt::warn!("[FW MGR] Not validated, ignoring apply cmd");
                        continue;
                    }

                    defmt::info!("[FW MGR] Trying to apply new firmware...");

                    let mut local_hash: Hash = [0u8; _];
                    let mut chunk_buf = [0u8; blake3::CHUNK_LEN];
                    if let Err(e) = self
                        .updater
                        .hash::<blake3::Hasher>(size as u32, &mut chunk_buf, &mut local_hash)
                        .await
                    {
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
                }
                Header::Reset => reset!(),
                Header::Invalid(t) => defmt::warn!("[FW MGR] Received invalid Header! [{}]", t),
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
