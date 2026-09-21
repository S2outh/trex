use core::sync::atomic::{AtomicU8, Ordering};

use south_common::{chell::ChellDefinition, definitions::groundstation::trex as defs};

use nalgebra as na;
use embassy_time::{Duration, Ticker};
use portable_atomic::AtomicF64;
use serde::Serialize;

use crate::{NATS_MAX_MESSAGE_SIZE, NATS_NUM_SUBS, NatsCollections, control_loop::axis::AxisState};
type NatsClient<'a> = embassy_nats::Client<'a, NatsCollections, NATS_NUM_SUBS>;

#[repr(u8)]
pub enum StateTM {
    Tracking,
    Manual,
}

pub struct AtomicState {
    state: AtomicU8,
}
impl AtomicState {
    pub const fn new(state: StateTM) -> Self {
        Self {
            state: AtomicU8::new(state as u8),
        }
    }
    pub fn store(&self, state: StateTM, order: Ordering) {
        self.state.store(state as u8, order);
    }
    pub fn load(&self, order: Ordering) -> StateTM {
        match self.state.load(order) {
            0 => StateTM::Tracking,
            1 => StateTM::Manual,
            _ => unreachable!(),
        }
    }
}

struct TM {
    az_pos: f64,
    az_vel: f64,
    el_pos: f64,
    el_vel: f64,
    state: StateTM,
}

pub struct TMChannel {
    az_pos: AtomicF64,
    az_vel: AtomicF64,
    el_pos: AtomicF64,
    el_vel: AtomicF64,
    state: AtomicState,
}

impl TMChannel {
    pub const fn new() -> Self {
        Self {
            az_pos: AtomicF64::new(0.),
            az_vel: AtomicF64::new(0.),
            el_pos: AtomicF64::new(0.),
            el_vel: AtomicF64::new(0.),
            state: AtomicState::new(StateTM::Manual),
        }
    }
    pub fn store_az(&self, s: AxisState, order: Ordering) {
        self.az_pos.store(s.pos, order);
        self.az_vel.store(s.vel, order);
    }
    pub fn store_el(&self, s: AxisState, order: Ordering) {
        self.el_pos.store(s.pos, order);
        self.el_vel.store(s.vel, order);
    }
    pub fn store_state(&self, s: StateTM, order: Ordering) {
        self.state.store(s, order);
    }
    fn load(&self, order: Ordering) -> TM {
        TM {
            az_pos: self.az_pos.load(order),
            az_vel: self.az_vel.load(order),
            el_pos: self.el_pos.load(order),
            el_vel: self.el_vel.load(order),
            state: self.state.load(order),
        }
    }
}

#[derive(Debug)]
pub struct EndOfStorage;

impl core::fmt::Display for EndOfStorage {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.write_str("end of storage")
    }
}

impl core::error::Error for EndOfStorage {}

struct WriteBuffer {
    pos: usize,
    storage: [u8; NATS_MAX_MESSAGE_SIZE],
}

impl WriteBuffer {
    fn new() -> Self {
        Self { pos: 0, storage: [0; _] }
    }
    fn as_bytes(&self) -> &[u8] {
        &self.storage[..self.pos]
    }
}

impl minicbor::encode::Write for WriteBuffer {
    type Error = EndOfStorage;
    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        if (self.storage.len() - self.pos) < buf.len() {
            return Err(EndOfStorage)
        }
        let n_pos = self.pos + buf.len();
        self.storage[self.pos..n_pos].copy_from_slice(buf);
        self.pos = n_pos;
        Ok(())
    }
}

pub struct TMLoop<'a> {
    nats_client: NatsClient<'a>,
    tm_channel: &'a TMChannel,
}

impl<'a> TMLoop<'a> {
    pub fn new(nats_client: NatsClient<'a>, tm_channel: &'a TMChannel) -> Self {
        Self {
            nats_client,
            tm_channel,
        }
    }
    pub async fn run(&mut self) -> ! {
        const TM_INTERVAL: Duration = Duration::from_millis(500);
        let mut tm_ticker = Ticker::every(TM_INTERVAL);
        loop {
            let tm = self.tm_channel.load(Ordering::Relaxed);
            
            // Silence warnings
            let _ = tm.az_vel;
            let _ = tm.el_vel;
            let _ = tm.state;

            let angles = na::Vector2::new(tm.az_pos, tm.el_pos);

            let mut storage = WriteBuffer::new();
            let mut serializer = minicbor_serde::Serializer::new(&mut storage);
            if angles.serialize(&mut serializer).is_ok() {
                self.nats_client.publish(
                    heapless::String::try_from(defs::Angles.address()).unwrap(),
                    heapless::vec::Vec::from_slice(storage.as_bytes()).unwrap(),
                ).await;
            }

            tm_ticker.next().await;
        }
    }
}
