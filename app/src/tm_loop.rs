use core::sync::atomic::{AtomicU8, Ordering};

use south_common::{
    chell::ChellDefinition, definitions::groundstation::trex as defs, types::trex::State as StateTM,
};

use embassy_time::{Duration, Instant, Ticker};
use nalgebra as na;
use portable_atomic::AtomicF64;
use serde::Serialize;

use crate::{NATS_MAX_MESSAGE_SIZE, NATS_NUM_SUBS, NatsCollections, control_loop::axis::AxisState};
type NatsClient<'a> = embassy_nats::Client<'a, NatsCollections, NATS_NUM_SUBS>;

pub struct AtomicState {
    state: AtomicU8,
}
impl AtomicState {
    const fn to_u8(state: StateTM) -> u8 {
        match state {
            StateTM::Tracking => 0,
            StateTM::Manual => 1,
        }
    }
    pub const fn new(state: StateTM) -> Self {
        Self {
            state: AtomicU8::new(Self::to_u8(state)),
        }
    }
    pub fn store(&self, state: StateTM, order: Ordering) {
        self.state.store(Self::to_u8(state), order);
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
    storage: heapless::Vec<u8, NATS_MAX_MESSAGE_SIZE>,
}

impl WriteBuffer {
    fn new() -> Self {
        Self {
            storage: heapless::Vec::new(),
        }
    }
    fn get_inner(self) -> heapless::Vec<u8, NATS_MAX_MESSAGE_SIZE> {
        self.storage
    }
}

impl minicbor::encode::Write for WriteBuffer {
    type Error = EndOfStorage;
    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        self.storage
            .extend_from_slice(buf)
            .map_err(|_| EndOfStorage)?;
        Ok(())
    }
}

pub struct TMLoop<'a> {
    nats_client: NatsClient<'a>,
    tm_channel: &'a TMChannel,
}

// This needs to be done via chell in the long run,
// but rn chell requires alloc :(
#[derive(serde::Serialize)]
struct TMValue<T> {
    timestamp: u64,
    value: T,
}

impl<'a> TMLoop<'a> {
    pub fn new(nats_client: NatsClient<'a>, tm_channel: &'a TMChannel) -> Self {
        Self {
            nats_client,
            tm_channel,
        }
    }
    async fn publish<T: serde::Serialize>(
        &mut self,
        def: &dyn ChellDefinition,
        timestamp: u64,
        value: T,
    ) {
        let mut storage = WriteBuffer::new();
        let mut serializer = minicbor_serde::Serializer::new(&mut storage);
        let tm_value = TMValue { timestamp, value };
        if tm_value.serialize(&mut serializer).is_ok() {
            self.nats_client
                .publish(
                    heapless::String::try_from(def.address()).unwrap(),
                    storage.get_inner(),
                )
                .await;
        }
    }
    pub async fn run(&mut self) -> ! {
        const TM_INTERVAL: Duration = Duration::from_millis(500);
        let mut tm_ticker = Ticker::every(TM_INTERVAL);
        loop {
            let tm = self.tm_channel.load(Ordering::Relaxed);
            let timestamp = Instant::now().as_micros();

            let angles = na::Vector2::new(tm.az_pos, tm.el_pos);
            self.publish(&defs::Angles, timestamp, angles).await;

            let angular_velocities = na::Vector2::new(tm.az_vel, tm.el_vel);
            self.publish(&defs::AngularVelocities, timestamp, angular_velocities)
                .await;

            self.publish(&defs::State, timestamp, tm.state).await;

            tm_ticker.next().await;
        }
    }
}
