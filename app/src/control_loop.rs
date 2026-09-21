pub mod axis;

use core::sync::atomic::Ordering;

use embassy_time::Instant;
use nalgebra as na;

use embassy_stm32::peripherals;
use embassy_stm32::timer;

use embassy_time::{Duration, Timer};
use south_common::chell::ChellDefinition;
use south_common::definitions::groundstation::trex as defs;
use south_common::types::trex::{self, Command};

use crate::tm_loop::StateTM;
use crate::tm_loop::TMChannel;
use crate::{NATS_NUM_SUBS, NatsCollections, control_loop::axis::Axis};

type NatsClient<'a> = embassy_nats::Client<'a, NatsCollections, NATS_NUM_SUBS>;
type NatsMsg = embassy_nats::NatsMsg<NatsCollections>;

type AzimutAxis<'a> = Axis<'a, peripherals::TIM3, peripherals::TIM24, timer::Ch3>;
type ElevationAxis<'a> = Axis<'a, peripherals::TIM2, peripherals::TIM23, timer::Ch1>;

const NATS_MSG_CHANNEL_SIZE: usize = 10;

static C_CH: embassy_nats::MsgChannel<NatsCollections, NATS_MSG_CHANNEL_SIZE> =
    embassy_nats::MsgChannel::new();

enum State {
    Tracking,
    Manual { target: na::Vector2<f64> },
}

pub struct ControlLoop<'a> {
    state: State,
    tm_channel: &'a TMChannel,
    last_tick: Instant,
    nats_client: NatsClient<'a>,
    azimut: AzimutAxis<'a>,
    elevation: ElevationAxis<'a>,
}

impl<'a> ControlLoop<'a> {
    pub fn new(
        nats_client: NatsClient<'a>,
        tm_channel: &'a TMChannel,
        azimut: AzimutAxis<'a>,
        elevation: ElevationAxis<'a>,
    ) -> Self {
        Self {
            nats_client,
            tm_channel,
            azimut,
            elevation,
            state: State::Manual {
                target: na::Vector2::zeros(),
            },
            last_tick: Instant::now(),
        }
    }
    // update the current target objective
    pub async fn update(&mut self, nats_msg: NatsMsg) {
        match minicbor_serde::from_slice::<Command>(&nats_msg.data) {
            Ok(cmd) => match cmd {
                Command::State(state_cmd) => {
                    self.state = match state_cmd {
                        trex::StateCommand::Tracking => State::Tracking,
                        trex::StateCommand::Manual => State::Manual {
                            target: na::Vector2::zeros(),
                        },
                    }
                }
                Command::Rotate(new_target) => {
                    let State::Manual { ref mut target } = self.state else {
                        return;
                    };
                    defmt::info!(
                        "[CTRL] setting new target: {} {}",
                        new_target.x,
                        new_target.y
                    );
                    *target = new_target;
                }
            },
            Err(e) => defmt::warn!("[CTRL] could not decode cmd: {}", defmt::Debug2Format(&e)),
        }
    }
    pub async fn run_tracking(&mut self, _dt: f64) {
        // TODO
        self.tm_channel.store_state(StateTM::Tracking, Ordering::Relaxed);
        Timer::after(Duration::from_millis(200)).await;
    }
    pub async fn run_manual(&mut self, target: na::Vector2<f64>, dt: f64) {
        // TEMP
        let az_state = self.azimut.update(target.x, dt);
        let el_state = self.elevation.update(target.y, dt);
        self.tm_channel.store_az(az_state, Ordering::Relaxed);
        self.tm_channel.store_el(el_state, Ordering::Relaxed);
        self.tm_channel.store_state(StateTM::Manual, Ordering::Relaxed);
        Timer::after_millis(5).await;
    }
    pub async fn run(&mut self) -> ! {
        self.nats_client
            .subscribe(
                heapless::String::try_from(defs::Command.address()).unwrap(),
                &C_CH,
            )
            .await
            .unwrap();

        loop {
            if let Some(msg) = self.nats_client.try_receive() {
                self.update(msg).await;
            }

            // TEMP, getting dt via embassy time
            let now = Instant::now();
            let dt = now.duration_since(self.last_tick);
            let dt_secs = dt.as_micros() as f64 / 1_000_000.;
            self.last_tick = now;

            match self.state {
                State::Tracking => self.run_tracking(dt_secs).await,
                State::Manual { target } => self.run_manual(target, dt_secs).await,
            }
        }
    }
}
