use embassy_net::tcp::{self, TcpSocket};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;

enum LogAction {
    Acquire,
    Release,
    Write(alloc::vec::Vec<u8>),
}

static LOG_CHANNEL: embassy_sync::channel::Channel<ThreadModeRawMutex, LogAction, 100> = embassy_sync::channel::Channel::new();

#[defmt::global_logger]
struct Logger;

unsafe impl defmt::Logger for Logger {
    fn acquire() {
        let _ = LOG_CHANNEL.try_send(LogAction::Acquire);
    }

    unsafe fn flush() {
        // ignore for now
    }

    unsafe fn release() {
        let _ = LOG_CHANNEL.try_send(LogAction::Release);
    }

    unsafe fn write(bytes: &[u8]) {
        let mut vec = alloc::vec![0; bytes.len()];
        vec.copy_from_slice(bytes);
        let _ = LOG_CHANNEL.try_send(LogAction::Write(vec));
    }
}

pub struct TcpEncoder<'a> {
    socket: TcpSocket<'a>,
    encoder: defmt::Encoder,
    port: u16,
}

impl<'a> TcpEncoder<'a> {
    pub async fn new(
        socket: TcpSocket<'a>,
        port: u16,
    ) -> Self {
        let encoder = defmt::Encoder::new();

        Self {
            socket,
            encoder,
            port,
        }
    }

    async fn write_buf(&mut self, buf: &[u8]) -> Result<(), tcp::Error> {
        let mut pos = 0;
        while pos < buf.len() {
            let bytes_written = self.socket.write(&buf[pos..]).await?;
            if bytes_written == 0 {
                return Err(tcp::Error::ConnectionReset);
            }
            pos += bytes_written;
        }
        Ok(())
    }

    async fn run_connected(&mut self) {
        loop {
            match LOG_CHANNEL.receive().await {
                LogAction::Acquire => {
                    let mut vec = alloc::vec::Vec::new();
                    self.encoder.start_frame(|b| {
                        vec.extend_from_slice(b);
                    });
                    if let Err(_) = self.write_buf(&vec).await {
                        return;
                    }
                },

                LogAction::Release => {
                    let mut vec = alloc::vec::Vec::new();
                    self.encoder.end_frame(|b| {
                        vec.extend_from_slice(b);
                    });
                    if let Err(_) = self.write_buf(&vec).await {
                        return;
                    }
                },

                LogAction::Write(bytes) => {
                    let mut vec = alloc::vec::Vec::new();
                    self.encoder.write(&bytes, |b| {
                        vec.extend_from_slice(b);
                    });
                    if let Err(_) = self.write_buf(&vec).await {
                        return;
                    }
                },
            }
        }
    }

    async fn disconnect(&mut self) {
        self.socket.abort();
        let _ = self.socket.flush().await;
    }
    pub async fn run(&mut self) -> ! {
        loop {
            if let Err(_) = self.socket.accept(self.port).await {
                self.disconnect().await;
                continue;
            }
            self.run_connected().await;
            self.disconnect().await;
        }
    }
}
