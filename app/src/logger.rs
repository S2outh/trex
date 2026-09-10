use core::cell::UnsafeCell;

use embassy_net::tcp::{self, TcpSocket};
use embassy_sync::blocking_mutex::raw::ThreadModeRawMutex;

#[defmt::global_logger]
struct Logger;

static ENCODER: TcpEncoder = TcpEncoder::new();

unsafe impl defmt::Logger for Logger {
    fn acquire() {
        ENCODER.acquire();
    }

    unsafe fn flush() {
        // ignore for now
    }

    unsafe fn release() {
        unsafe {
            ENCODER.release();
        }
    }

    unsafe fn write(bytes: &[u8]) {
        unsafe {
            ENCODER.write(bytes);
        }
    }
}

static LOG_CHANNEL: embassy_sync::channel::Channel<ThreadModeRawMutex, alloc::vec::Vec<u8>, 64> =
    embassy_sync::channel::Channel::new();

struct TcpEncoder {
    encoder: UnsafeCell<defmt::Encoder>,
}
unsafe impl Sync for TcpEncoder {}

impl TcpEncoder {
    const fn new() -> Self {
        let encoder = UnsafeCell::new(defmt::Encoder::new());
        Self { encoder }
    }

    fn acquire(&self) {
        let mut vec = alloc::vec::Vec::new();
        unsafe {
            self.encoder.get().as_mut().unwrap().start_frame(|b| {
                vec.extend_from_slice(b);
            });
        }
        let _ = LOG_CHANNEL.try_send(vec);
    }

    unsafe fn release(&self) {
        let mut vec = alloc::vec::Vec::new();
        unsafe {
            self.encoder.get().as_mut().unwrap().end_frame(|b| {
                vec.extend_from_slice(b);
            });
        }
        let _ = LOG_CHANNEL.try_send(vec);
    }

    unsafe fn write(&self, bytes: &[u8]) {
        let mut vec = alloc::vec::Vec::new();
        unsafe {
            self.encoder.get().as_mut().unwrap().write(bytes, |b| {
                vec.extend_from_slice(b);
            });
        }
        let _ = LOG_CHANNEL.try_send(vec);
    }
}

pub struct TcpLogger<'a> {
    socket: TcpSocket<'a>,
    port: u16,
}

impl<'a> TcpLogger<'a> {
    pub async fn new(socket: TcpSocket<'a>, port: u16) -> Self {
        Self { socket, port }
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
            let vec = LOG_CHANNEL.receive().await;

            if let Err(_) = self.write_buf(&vec).await {
                return;
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
