pub mod axis;

use embassy_stm32::timer;
use embassy_stm32::peripherals;

use embassy_time::{Duration, Timer};
use south_common::chell::ChellDefinition;
use south_common::types::trex::Command;
use south_common::definitions::groundstation::trex as defs;

use crate::{NATS_NUM_SUBS, NatsCollections, control_loop::axis::Axis};

type NatsClient<'a> = embassy_nats::Client<'a, NatsCollections, NATS_NUM_SUBS>;
type AzimutAxis<'a> = Axis<'a, peripherals::TIM3, peripherals::TIM24, timer::Ch3>;
type ElevationAxis<'a> = Axis<'a, peripherals::TIM2, peripherals::TIM23, timer::Ch1>;

const NATS_MSG_CHANNEL_SIZE: usize = 10;

static TC_CH: embassy_nats::MsgChannel<NatsCollections, NATS_MSG_CHANNEL_SIZE> =
    embassy_nats::MsgChannel::new();


pub struct ControlLoop<'a> {
    nats_client: NatsClient<'a>,
    azimut: AzimutAxis<'a>,
    elevation: ElevationAxis<'a>,
}

impl<'a> ControlLoop<'a> {
    pub async fn new(
        nats_client: NatsClient<'a>,
        azimut: AzimutAxis<'a>,
        elevation: ElevationAxis<'a>,
    ) -> Self {


        Self {
            nats_client,
            azimut,
            elevation,
        }
    }
    pub async fn run(&mut self) -> ! {
        self.nats_client
            .subscribe(
                heapless::String::try_from(defs::Command.address()).unwrap(),
                &TC_CH,
            )
            .await
            .unwrap();

        loop {
            let nats_msg = self.nats_client.receive().await;
            match minicbor_serde::from_slice::<Command>(&nats_msg.data) {
                Ok(cmd) => {
                    match cmd {
                        Command::RotateAz(v) => {
                            defmt::info!("azimut pos before move: {}", self.azimut.get_pos());
                            self.azimut.set_speed(v as f64);
                            Timer::after(Duration::from_secs(1)).await;
                            self.azimut.stop();
                            defmt::info!("azimut pos after move: {}", self.azimut.get_pos());
                        },
                        Command::RotateEl(v) => {
                            defmt::info!("elevation pos before move: {}", self.elevation.get_pos());
                            self.elevation.set_speed(v as f64);
                            Timer::after(Duration::from_secs(1)).await;
                            self.elevation.stop();
                            defmt::info!("elevation pos after move: {}", self.elevation.get_pos());
                        },
                        _ => defmt::warn!("unimplemented"),
                    }
                }
                Err(e) => defmt::warn!("could not decode cmd: {}", defmt::Debug2Format(&e)),
            }
        }
    }
}
