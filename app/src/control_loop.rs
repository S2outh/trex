pub mod axis;

use core::sync::atomic::Ordering;

use embassy_time::Instant;
use nalgebra as na;

use embassy_stm32::peripherals;
use embassy_stm32::timer;

use embassy_time::Timer;
use south_common::chell::ChellDefinition;
use south_common::definitions::groundstation as gcs_defs;
use south_common::definitions::telemetry as tm;
use south_common::types::trex::{self, Command};

use crate::tm_loop::AxisState;
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
    Halted,
    Tracking(TrackingLoop<'a>),
    Manual(ManualLoop<'a>),
}

struct TrackingLoop<'a> {
    nats_client: NatsClient<'a>,
}

impl<'a> TrackingLoop<'a> {
    async fn init(mut nats_client: NatsClient<'a>) -> Self {
        nats_client
            .subscribe(
                heapless::String::try_from(tm::upper_sensor::KinematicState.address()).unwrap(),
                &C_CH,
            )
            .await
            .unwrap();

        Self { nats_client }
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
    }
}

struct ManualLoop<'a> {
    target_pos: na::Vector2<f64>,
    target_vel: na::Vector2<f64>,
    nats_client: NatsClient<'a>,
}

impl<'a> ManualLoop<'a> {
    async fn init(mut nats_client: NatsClient<'a>) -> Self {
        nats_client
            .subscribe(
                heapless::String::try_from(gcs_defs::trex::Command.address()).unwrap(),
                &C_CH,
            )
            .await
            .unwrap();

        Self {
            target_pos: na::Vector2::zeros(),
            target_vel: na::Vector2::zeros(),
            nats_client,
        }
    }
    // Execute a received command in manual mode
    async fn process_command(&mut self, axs: &mut Axs<'a>, nats_msg: NatsMsg) {
        match minicbor_serde::from_slice::<Command>(&nats_msg.data) {
            Ok(cmd) => match cmd {
                Command::Set(pos) => {
                    defmt::info!("[CTRL] setting new target: {} {}", pos.x, pos.y);
                    self.target_pos = pos;
                    self.target_vel = na::Vector2::zeros();
                }
                Command::SetMoving(pos, vel) => {
                    defmt::info!(
                        "[CTRL] setting new moving target, pos: {} {}, vel: {} {}",
                        pos.x,
                        pos.y,
                        vel.x,
                        vel.y
                    );
                    self.target_pos = pos;
                    self.target_vel = vel;
                }
                Command::Rotate(rel_pos) => {
                    defmt::info!(
                        "[CTRL] setting new relative target: {} {}",
                        rel_pos.x,
                        rel_pos.y
                    );
                    let pos = na::Vector2::new(axs.azimut.get_pos(), axs.elevation.get_pos());
                    self.target_pos = pos + rel_pos;
                    self.target_vel = na::Vector2::zeros();
                }
                Command::RotateMoving(rel_pos, rel_vel) => {
                    defmt::info!(
                        "[CTRL] setting new moving relative target, pos: {} {}, vel: {} {}",
                        rel_pos.x,
                        rel_pos.y,
                        rel_vel.x,
                        rel_vel.y
                    );
                    let pos = na::Vector2::new(axs.azimut.get_pos(), axs.elevation.get_pos());
                    let vel = na::Vector2::new(axs.azimut.get_vel(), axs.elevation.get_vel());
                    self.target_pos = pos + rel_pos;
                    self.target_vel = vel + rel_vel;
                }
            },
            Err(e) => defmt::warn!("[CTRL] could not decode cmd: {}", defmt::Debug2Format(&e)),
        }
    }
    async fn tick(&mut self, axs: &mut Axs<'a>, dt: f64) {
        if let Some(cmd) = self.nats_client.try_receive() {
            self.process_command(axs, cmd).await;
        }

        axs.azimut.update(self.target_pos.x, self.target_vel.x, dt);
        axs.elevation
            .update(self.target_pos.y, self.target_vel.y, dt);
    }
}

pub struct ControlLoop<'a> {
    state: State<'a>,
    tm_channel: &'a TMChannel,
    last_tick: Instant,
    state_client: NatsClient<'a>,
    axs: Axs<'a>,
}

impl<'a> ControlLoop<'a> {
    pub fn new(
        nats_client: NatsClient<'a>,
        tm_channel: &'a TMChannel,
        azimut: AzimutAxis<'a>,
        elevation: ElevationAxis<'a>,
    ) -> Self {
        Self {
            state: State::Halted,
            tm_channel,
            last_tick: Instant::now(),
            state_client: nats_client.clone(),
            axs: Axs { azimut, elevation },
        }
    }
    // Update the state after a received state change command
    async fn process_state(&mut self, nats_msg: NatsMsg) {
        match minicbor_serde::from_slice::<trex::State>(&nats_msg.data) {
            Ok(cmd) => {
                match cmd {
                    trex::State::Halted => {
                        defmt::info!("new state: halted");
                        self.axs.azimut.stop();
                        self.axs.elevation.stop();
                        self.state = State::Halted;
                    }
                    trex::State::Tracking => {
                        defmt::info!("new state: tracking");
                        // set state to halted first to free the nats channel
                        self.state = State::Halted;
                        self.state =
                            State::Tracking(TrackingLoop::init(self.state_client.clone()).await);
                    }
                    trex::State::Manual => {
                        defmt::info!("new state: manual");
                        // set state to halted first to free the nats channel
                        self.state = State::Halted;
                        self.state =
                            State::Manual(ManualLoop::init(self.state_client.clone()).await);
                    }
                }
                self.tm_channel.store_state(cmd, Ordering::Relaxed);
            }
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

        loop {
            if let Some(state_cmd) = self.state_client.try_receive() {
                self.process_state(state_cmd).await;
            }

            // TEMP, getting dt via embassy time
            let now = Instant::now();
            let dt = now.duration_since(self.last_tick);
            let dt_secs = dt.as_micros() as f64 / 1_000_000.;
            self.last_tick = now;

            // send axis tm (every tick for now)
            self.tm_channel.store_az(
                AxisState {
                    pos: self.axs.azimut.get_pos(),
                    vel: self.axs.azimut.get_vel(),
                },
                Ordering::Relaxed,
            );
            self.tm_channel.store_el(
                AxisState {
                    pos: self.axs.elevation.get_pos(),
                    vel: self.axs.elevation.get_vel(),
                },
                Ordering::Relaxed,
            );

            match &mut self.state {
                State::Halted => (),
                State::Tracking(tl) => tl.tick(dt_secs).await,
                State::Manual(ml) => ml.tick(&mut self.axs, dt_secs).await,
            }

            Timer::after_millis(5).await
        }
    }
}
