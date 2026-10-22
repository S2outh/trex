use embassy_boot_stm32::{AlignedBuffer, FirmwareUpdater, FirmwareUpdaterConfig, State};
use embassy_futures::yield_now;
use embassy_net::tcp::{self, TcpListener, TcpSocket};
use embassy_stm32::{
    flash::{Flash, WRITE_SIZE},
    mode::Async,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};
use trex_firmware_transport::*;

type Partition<'a> = embassy_embedded_hal::flash::partition::Partition<
    'a,
    NoopRawMutex,
    embassy_stm32::flash::Flash<'a, Async>,
>;
type MtxFlash<'a> = Mutex<NoopRawMutex, Flash<'a, Async>>;

const PUBLIC_KEY: &'static [u8] = include_bytes!("keys/public.key");

pub struct FirmwareManagerStorage<'a> {
    flash: MtxFlash<'a>,
    magic: AlignedBuffer<WRITE_SIZE>,
}

impl<'a> FirmwareManagerStorage<'a> {
    pub fn new(flash: Flash<'a, Async>) -> Self {
        let flash = Mutex::new(flash);
        let magic = AlignedBuffer([0; WRITE_SIZE]);
        Self { flash, magic }
    }
}

pub struct FirmwareManager<'a> {
    updater: FirmwareUpdater<'a, Partition<'a>, Partition<'a>>,
    listener: TcpListener<'a>,
    socket: TcpSocket<'a, 'a>,
    validated: bool,
}

impl<'a> FirmwareManager<'a> {
    pub async fn new(
        storage: &'a mut FirmwareManagerStorage<'a>,
        listener: TcpListener<'a>,
        socket: TcpSocket<'a, 'a>,
    ) -> Self {
        let config = FirmwareUpdaterConfig::from_linkerfile(&storage.flash, &storage.flash);
        let mut updater = FirmwareUpdater::new(config, &mut storage.magic.0);

        let state = updater.get_state().await.unwrap();
        let validated = if let State::Swap = state { false } else { true };

        defmt::info!("[FW MGR] Initializing firmware manager");

        match state {
            State::Boot => defmt::info!("[FW MGR] Booting stable firmware"),
            State::Swap => defmt::warn!("[FW MGR] Booting new firmware"),
            State::Revert => defmt::error!("[FW MGR] Booting reverted firmware"),
            State::DfuDetach => defmt::error!("[FW MGR] DFU Detatch mode"),
        }

        Self {
            updater,
            listener,
            socket,
            validated,
        }
    }
    async fn try_validate(&mut self) -> bool {
        if !self.validated {
            defmt::info!("[FW MGR] Firmware validated");
            self.validated = true;
            self.updater.mark_booted().await.unwrap();
            true
        } else {
            false
        }
    }
    async fn read_all(&mut self, mut buf: &mut [u8]) -> Result<(), tcp::Error> {
        while !buf.is_empty() {
            let bytes_read = self.socket.read(buf).await?;
            buf = &mut buf[bytes_read..];
            if bytes_read == 0 {
                return Err(tcp::Error::ConnectionReset);
            }
            // Yield here to not block in case of large blocks of data
            yield_now().await;
        }
        Ok(())
    }

    async fn write_all(&mut self, mut buf: &[u8]) -> Result<(), tcp::Error> {
        while !buf.is_empty() {
            let bytes_written = self.socket.write(buf).await?;
            buf = &buf[bytes_written..];
            if bytes_written == 0 {
                return Err(tcp::Error::ConnectionReset);
            }
            // Yield here to not block in case of large blocks of data
            yield_now().await;
        }
        Ok(())
    }
    async fn respond(&mut self, response: Response) -> Result<(), tcp::Error> {
        if let Err(e) = Serializer::new(async |a| self.write_all(a).await)
            .write_response(response)
            .await
        {
            defmt::error!("could not send response: {}", e);
        }
        Ok(())
    }
    async fn run_connected(&mut self) {
        loop {
            let Ok(header) = Deserializer::new(async |a| self.read_all(a).await)
                .read_header()
                .await
            else {
                return;
            };
            match header {
                Header::Validate => {
                    if self.try_validate().await {
                        if self.respond(Response::Success).await.is_err() {
                            return;
                        }
                    } else {
                        if self.respond(Response::Fail).await.is_err() {
                            return;
                        }
                    }
                }
                Header::Chunk { offset, size } => {
                    defmt::info!(
                        "[FW MGR] Received chunk, offset: {}, size: {}",
                        offset,
                        size
                    );

                    let mut chunk = AlignedBuffer([0; CHUNK_SIZE]);
                    if self.read_all(&mut chunk.as_mut()[..size]).await.is_err() {
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
                Header::Apply { size, signature } => {
                    if !self.validated {
                        defmt::warn!("[FW MGR] Not validated, ignoring apply cmd");
                        if self.respond(Response::Fail).await.is_err() {
                            return;
                        }
                        continue;
                    }

                    defmt::info!("[FW MGR] Trying to apply new firmware...");

                    const HASH_CHUNK_SIZE: usize = 64;
                    let mut chunk_buf = AlignedBuffer([0u8; HASH_CHUNK_SIZE]);
                    if let Err(e) = self
                        .updater
                        .verify_and_mark_updated::<embassy_crypto::Sha512, embassy_crypto::ed25519::VerifyingKey>(
                            PUBLIC_KEY,
                            &signature,
                            chunk_buf.as_mut(),
                            size as u32,
                        ).await
                    {
                        defmt::error!("[FW MGR] Could not verify firmware: {}", e);
                        if self.respond(Response::Fail).await.is_err() {
                            return;
                        }
                        continue;
                    }

                    if self.respond(Response::Success).await.is_err() {
                        return;
                    }

                    defmt::info!("[FW MGR] Switching to new firmware...");

                    // Wait for the tcp socket to flush
                    Timer::after(Duration::from_secs(1)).await;
                    cortex_m::peripheral::SCB::sys_reset();
                }
                Header::Reset => {
                    if self.respond(Response::Success).await.is_err() {
                        return;
                    }
                    // Wait for the tcp socket to flush
                    Timer::after(Duration::from_secs(1)).await;
                    cortex_m::peripheral::SCB::sys_reset();
                }
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
            let Ok(token) = self.listener.accept().await else {
                continue;
            };
            if self.socket.accept(token).await.is_err() {
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
