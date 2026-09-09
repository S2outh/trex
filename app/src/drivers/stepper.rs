use core::f64::consts;

use embassy_stm32::{
    gpio::Output,
    time::hz,
    timer::{GeneralInstance4Channel, GeneralInstance32bit4Channel, TimerChannel},
};

pub mod step_counter;
pub mod step_interface;

use crate::drivers::stepper::{step_counter::StepCounter, step_interface::StepInterface};

pub struct Stepper<'d, T: GeneralInstance4Channel, TC: GeneralInstance32bit4Channel, C> {
    step_interface: StepInterface<'d, T, C>,
    step_counter: StepCounter<'d, TC>,
    dir: Output<'d>,
    _disable: Output<'d>,

    angle_factor: f64,
}

impl<'d, T: GeneralInstance4Channel, TC: GeneralInstance32bit4Channel, C: TimerChannel>
    Stepper<'d, T, TC, C>
{
    pub fn new(
        step_interface: StepInterface<'d, T, C>,
        step_counter: StepCounter<'d, TC>,
        dir: Output<'d>,
        mut disable: Output<'d>,
        steps_per_rev: u32,
    ) -> Self {
        let angle_factor = steps_per_rev as f64 / (2. * consts::PI);
        disable.set_low();
        Self {
            step_interface,
            step_counter,
            dir,
            _disable: disable,
            angle_factor,
        }
    }

    pub fn set_speed(&mut self, speed: f64) {
        let frequency = speed * self.angle_factor;
        self.set_dir(frequency);
        let frequency = frequency.abs();
        self.step_interface.set_frequency(hz(frequency as u32));
        self.step_interface.start();
    }

    pub fn stop(&mut self) {
        self.step_interface.stop();
    }

    pub fn get_steps(&mut self) -> u32 {
        self.step_counter.get()
    }

    fn set_dir(&mut self, dir: f64) {
        if dir > 0. {
            self.dir.set_high();
        } else {
            self.dir.set_low();
        }
    }
}
