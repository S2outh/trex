pub mod axis;

use core::sync::atomic::Ordering;

use embassy_time::Instant;
use nalgebra as na;

use embassy_stm32::peripherals;
use embassy_stm32::timer;

use embassy_time::{Duration, Timer};
use south_common::chell::ChellDefinition;
use south_common::definitions::groundstation as gcs_defs;
use south_common::definitions::telemetry as tm;
use south_common::types::trex::{self, Command};

use crate::tm_loop::TMChannel;
use crate::{NATS_NUM_SUBS, NatsCollections, control_loop::axis::Axis};

type NatsClient<'a> = embassy_nats::Client<'a, NatsCollections, NATS_NUM_SUBS>;
type NatsMsg = embassy_nats::NatsMsg<NatsCollections>;

type AzimutAxis<'a> = Axis<'a, peripherals::TIM3, peripherals::TIM24, timer::Ch3>;
type ElevationAxis<'a> = Axis<'a, peripherals::TIM2, peripherals::TIM23, timer::Ch1>;

const NATS_MSG_CHANNEL_SIZE: usize = 3;

static S_CH: embassy_nats::MsgChannel<NatsCollections, NATS_MSG_CHANNEL_SIZE> =
    embassy_nats::MsgChannel::new();

static C_CH: embassy_nats::MsgChannel<NatsCollections, NATS_MSG_CHANNEL_SIZE> =
    embassy_nats::MsgChannel::new();

struct Axs<'a> {
    azimut: AzimutAxis<'a>,
    elevation: ElevationAxis<'a>,
}

enum State<'a> {
    Tracking(TrackingLoop<'a>),
    Manual(ManualLoop<'a>),
}

struct TrackingLoop<'a> {
    tm_channel: &'a TMChannel,
    nats_client: NatsClient<'a>,
}

impl<'a> TrackingLoop<'a> {
    fn new(tm_channel: &'a TMChannel, nats_client: NatsClient<'a>) -> Self {
        Self { tm_channel, nats_client }
    }
    async fn init(&mut self) {
        self.nats_client
            .subscribe(
                heapless::String::try_from(tm::upper_sensor::KinematicState.address()).unwrap(),
                &C_CH,
            )
            .await
            .unwrap();
    }
    // Process a received state in tracking mode
    async fn process_target(&mut self, _nats_msg: NatsMsg) {
        defmt::info!("new target");
        // TODO
    }

    async fn tick(&mut self, _dt: f64) {
        if let Some(target) = self.nats_client.try_receive() {
            self.process_target(target).await;
        }

        // TODO
        self.tm_channel
            .store_state(trex::State::Tracking, Ordering::Relaxed);
        Timer::after(Duration::from_millis(200)).await;
    }
}

struct ManualLoop<'a> {
    target: na::Vector2<f64>,
    tm_channel: &'a TMChannel,
    nats_client: NatsClient<'a>,
}

impl<'a> ManualLoop<'a> {
    fn new(tm_channel: &'a TMChannel, nats_client: NatsClient<'a>) -> Self {
        Self { target: na::Vector2::zeros(), tm_channel, nats_client }
    }
    async fn init(&mut self) {
        self.nats_client
            .subscribe(
                heapless::String::try_from(gcs_defs::trex::Command.address()).unwrap(),
                &C_CH,
            )
            .await
            .unwrap();
    }
    // Execute a received command in manual mode
    async fn process_command(&mut self, axs: &mut Axs<'a>, nats_msg: NatsMsg) {
        match minicbor_serde::from_slice::<Command>(&nats_msg.data) {
            Ok(cmd) => match cmd {
                Command::Set(new_target) => {
                    defmt::info!(
                        "[CTRL] setting new target: {} {}",
                        new_target.x,
                        new_target.y
                    );
                    self.target = new_target;
                }
                Command::Rotate(rel) => {
                    defmt::info!(
                        "[CTRL] setting new relative target: {} {}",
                        rel.x,
                        rel.y
                    );
                    let pos = na::Vector2::new(axs.azimut.get_pos(), axs.elevation.get_pos());
                    self.target = pos + rel;
                }
            },
            Err(e) => defmt::warn!("[CTRL] could not decode cmd: {}", defmt::Debug2Format(&e)),
        }
    }
    async fn tick(&mut self, axs: &mut Axs<'a>, dt: f64) {
        if let Some(cmd) = self.nats_client.try_receive() {
            self.process_command(axs, cmd).await;
        }

        // TEMP
        let az_state = axs.azimut.update(self.target.x, 0., dt);
        let el_state = axs.elevation.update(self.target.y, 0., dt);
        self.tm_channel.store_az(az_state, Ordering::Relaxed);
        self.tm_channel.store_el(el_state, Ordering::Relaxed);
        self.tm_channel
            .store_state(trex::State::Manual, Ordering::Relaxed);
        Timer::after_millis(5).await;
    }
}

pub struct ControlLoop<'a> {
    state: State<'a>,
    tm_channel: &'a TMChannel,
    last_tick: Instant,
    state_client: NatsClient<'a>,
    axs: Axs<'a>
}

impl<'a> ControlLoop<'a> {
    pub fn new(
        nats_client: NatsClient<'a>,
        tm_channel: &'a TMChannel,
        azimut: AzimutAxis<'a>,
        elevation: ElevationAxis<'a>,
    ) -> Self {
        Self {
            state_client: nats_client.clone(),
            tm_channel,
            axs: Axs { azimut, elevation },
            state: State::Tracking(TrackingLoop::new(tm_channel, nats_client)),
            last_tick: Instant::now(),
        }
    }
    // Update the state after a received state change command
    async fn process_state(&mut self, nats_msg: NatsMsg) {
        match minicbor_serde::from_slice::<trex::State>(&nats_msg.data) {
            Ok(cmd) => match cmd {
                trex::State::Tracking => {
                    defmt::info!("new state: tracking");
                    self.state = State::Tracking(TrackingLoop::new(self.tm_channel, self.state_client.clone()));
                    let State::Tracking(t) = &mut self.state else { panic!() };
                    t.init().await;
                },
                trex::State::Manual => {
                    defmt::info!("new state: manual");
                    self.state = State::Manual(ManualLoop::new(self.tm_channel, self.state_client.clone()));
                    let State::Manual(t) = &mut self.state else { panic!() };
                    t.init().await;
                },
            },
            Err(e) => defmt::warn!("[CTRL] could not decode state: {}", defmt::Debug2Format(&e)),
        }
    }
    pub async fn run(&mut self) -> ! {
        self.state_client
            .subscribe(
                heapless::String::try_from(gcs_defs::trex::ChangeState.address()).unwrap(),
                &S_CH,
            )
            .await
            .unwrap();

        let State::Tracking(t) = &mut self.state else { panic!() };
        t.init().await;

        loop {
            if let Some(state_cmd) = self.state_client.try_receive() {
                self.process_state(state_cmd).await;
            }

            // TEMP, getting dt via embassy time
            let now = Instant::now();
            let dt = now.duration_since(self.last_tick);
            let dt_secs = dt.as_micros() as f64 / 1_000_000.;
            self.last_tick = now;

            match &mut self.state {
                State::Tracking(tl) => tl.tick(dt_secs).await,
                State::Manual(ml) => ml.tick(&mut self.axs, dt_secs).await,
            }
        }
    }
}
