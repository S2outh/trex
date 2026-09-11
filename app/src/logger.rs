use core::{cell::{Cell, UnsafeCell}, sync::atomic::Ordering};

use critical_section::RestoreState;
use embassy_net::tcp::{self, TcpSocket};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, pipe::Pipe};
use embassy_time::Duration;
use portable_atomic::AtomicU32;

const MAX_FRAME_SIZE: usize = 512;

const LOG_BUF_SIZE: usize = 4096;
static LOG_PIPE: Pipe<CriticalSectionRawMutex, LOG_BUF_SIZE> = Pipe::new();

static DROPPED_FRAMES: AtomicU32 = AtomicU32::new(0);

static ENCODER: TcpEncoder = TcpEncoder::new();

#[defmt::global_logger]
struct Logger;

unsafe impl defmt::Logger for Logger {
    fn acquire() {
        ENCODER.acquire();
    }

    unsafe fn flush() {
        // ignore, since tcp can't do a blocking flush
        // (it requires net_task to run)
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

struct TcpEncoder {
    encoder: UnsafeCell<defmt::Encoder>,
    // restore state of the critical section
    restore: UnsafeCell<RestoreState>,
    // defmt requires acquire() to reject re-entrant access
    taken: Cell<bool>,
    // holds the current frame
    frame: UnsafeCell<[u8; MAX_FRAME_SIZE]>,
    // true if the frame is bigger than MAX_FRAME_SIZE
    frame_broken: Cell<bool>,
    // current frame ptr
    ptr: Cell<usize>,
}

// SAFETY: every field is only touched between acquire and release,
// and therefore inside of a critical section
unsafe impl Sync for TcpEncoder {}

impl TcpEncoder {
    const fn new() -> Self {
        Self {
            encoder: UnsafeCell::new(defmt::Encoder::new()),
            restore: UnsafeCell::new(RestoreState::invalid()),
            taken: Cell::new(false),
            frame: UnsafeCell::new([0; _]),
            frame_broken: Cell::new(false),
            ptr: Cell::new(0),
        }
    }

    fn acquire(&self) {
        // SAFETY: paired with release in TcpEncoder::Release
        // Everything between this and the end of the release() function
        // can't be interrupted
        let restore = unsafe { critical_section::acquire() };

        if self.taken.get() {
            unsafe { critical_section::release(restore) };
            panic!("defmt logger acquired re-entrantly");
        }
        self.taken.set(true);
        self.ptr.set(0);
        self.frame_broken.set(false);
        unsafe {
            *self.restore.get() = restore;
            self.encoder.get()
                .as_mut_unchecked()
                .start_frame(|b| self.ingress_bytes(b));
        }
    }

    unsafe fn write(&self, bytes: &[u8]) {
        unsafe {
            self.encoder.get()
                .as_mut_unchecked()
                .write(bytes, |b| self.ingress_bytes(b));
        }
    }

    unsafe fn release(&self) {
        unsafe {
            self.encoder.get()
                .as_mut_unchecked()
                .end_frame(|b| self.ingress_bytes(b));
        }

        if self.frame_broken.get() || self.ptr.get() > LOG_PIPE.free_capacity() {
            DROPPED_FRAMES.fetch_add(1, Ordering::Relaxed);
        } else {
            // write frame to LOG_BUF
            let frame = unsafe { self.frame.get().as_ref_unchecked() };
            let mut buf = &frame[..self.ptr.get()];
            while !buf.is_empty() {
                let n = LOG_PIPE.try_write(buf).unwrap();
                buf = &buf[n..];
            }
        }

        self.taken.set(false);
        unsafe {
            let restore = *self.restore.get();
            critical_section::release(restore);
        }
    }

    fn ingress_bytes(&self, buf: &[u8]) {
        if self.frame_broken.get() {
            return;
        }

        let ptr = self.ptr.get();
        if MAX_FRAME_SIZE - ptr < buf.len() {
            self.frame_broken.set(true);
            return
        }

        unsafe {
            self.frame.get().as_mut_unchecked()
                [ptr..]
                [..buf.len()]
                .copy_from_slice(buf);
        }
        self.ptr.set(ptr + buf.len());
    }
}

pub struct TcpLogger<'a> {
    socket: TcpSocket<'a>,
    port: u16,
}

impl<'a> TcpLogger<'a> {
    pub fn new(socket: TcpSocket<'a>, port: u16) -> Self {
        Self { socket, port }
    }

    async fn write_buf(&mut self, mut buf: &[u8]) -> Result<(), tcp::Error> {
        while !buf.is_empty() {
            let bytes_written = self.socket.write(buf).await?;
            buf = &buf[bytes_written..];
            if bytes_written == 0 {
                return Err(tcp::Error::ConnectionReset);
            }
        }
        Ok(())
    }

    async fn run_connected(&mut self) {
        let mut buf = [0; 512];
        let dropped_frames = DROPPED_FRAMES.load(Ordering::Relaxed);
        if dropped_frames > 0 {
            defmt::warn!("Dropped frames: {}", dropped_frames);
        }
        loop {
            let n = LOG_PIPE.read(&mut buf).await;
            if self.write_buf(&buf[..n]).await.is_err() {
                return;
            }
        }
    }

    async fn disconnect(&mut self) {
        LOG_PIPE.clear();
        self.socket.abort();
        let _ = self.socket.flush().await;
    }

    pub async fn run(&mut self) -> ! {
        self.socket.set_keep_alive(Some(Duration::from_secs(5)));
        self.socket.set_timeout(Some(Duration::from_secs(6)));
        loop {
            if self.socket.accept(self.port).await.is_err() {
                self.disconnect().await;
                continue;
            }
            self.run_connected().await;
            self.disconnect().await;
        }
    }
}
