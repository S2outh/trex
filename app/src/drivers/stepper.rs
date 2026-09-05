
use core::f64::consts;

use embassy_stm32::{Peri, gpio::Output, time::hz, timer::{GeneralInstance4Channel, TimerChannel}};

pub mod step_interface;
mod step_counter;

use crate::drivers::stepper::step_interface::{StepInterface, PulsePin};

pub struct Stepper<'d, T: GeneralInstance4Channel, C> {
    step_interface: StepInterface<'d, T, C>,
    dir: Output<'d>,
    enable: Output<'d>,

    angle_factor: f64,
}

impl<'d, T: GeneralInstance4Channel, C: TimerChannel> Stepper<'d, T, C> {
    pub fn new(timer: Peri<'d, T>, step: PulsePin<'d, T, C>, dir: Output<'d>, enable: Output<'d>, steps_per_rev: u32) -> Self {
        let step_interface = StepInterface::new(timer, step);
        let angle_factor = steps_per_rev as f64 / (2. * consts::PI);
        Self {
            step_interface,
            dir,
            enable,
            angle_factor,
        }
    }

    pub fn set_speed(&mut self, speed: f64) {
        let frequency = speed * self.angle_factor;
        self.set_dir(frequency);
        let frequency = frequency.abs();
        self.step_interface
            .set_frequency(hz(frequency as u32));
        self.step_interface.start();
        self.enable.set_high();
    }

    pub fn stop(&mut self) {
        self.step_interface.stop();
        self.enable.set_low();
    }

    fn set_dir(&mut self, dir: f64) {
        if dir > 0. {
            self.dir.set_high();
        } else {
            self.dir.set_low();
        }
    }
}
