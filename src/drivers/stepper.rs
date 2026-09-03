
use embassy_stm32::{Peri, gpio::Output, time::khz, timer::{GeneralInstance4Channel, TimerChannel}};

pub mod pulse_generator;
use crate::drivers::stepper::pulse_generator::{PulseGenerator, PulsePin};

struct Stepper<'d, T: GeneralInstance4Channel, C> {
    step_ctrl: PulseGenerator<'d, T, C>,
    dir: Output<'d>,
    enable: Output<'d>,
}

impl<'d, T: GeneralInstance4Channel, C: TimerChannel> Stepper<'d, T, C> {
    pub fn new(timer: Peri<'d, T>, step: PulsePin<'d, T, C>, dir: Output<'d>, enable: Output<'d>) -> Self {
        let step_ctrl = PulseGenerator::new(timer, step, khz(1));
        Self {
            step_ctrl,
            dir,
            enable,
        }
    }
}
