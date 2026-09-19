use core::f64::consts;

use embassy_stm32::{
    gpio::{Level, Output},
    time::hz,
    timer::{GeneralInstance4Channel, TimerChannel},
};

pub mod step_counter;
pub mod step_interface;

use crate::drivers::stepper::{step_counter::StepCounter, step_interface::StepInterface};

// A direction enum representing clockwise and counterclockwise directions
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Dir {
    Cw,
    Ccw,
}

impl Dir {
    fn from_float(v: f64) -> Self {
        if v > 0. { Dir::Cw } else { Dir::Ccw }
    }
    fn int(&self) -> i32 {
        match self {
            Dir::Cw => 1,
            Dir::Ccw => -1,
        }
    }
    fn float(&self) -> f64 {
        match self {
            Dir::Cw => 1.,
            Dir::Ccw => -1.,
        }
    }
    fn level(&self) -> Level {
        match self {
            Dir::Cw => Level::High,
            Dir::Ccw => Level::Low,
        }
    }
}

// Interface to a closed loop stepper motor,
// including step interface and step counter for accurate position estimation
pub struct Stepper<'d, T: GeneralInstance4Channel, TC: GeneralInstance4Channel, C> {
    step_interface: StepInterface<'d, T, C>,
    step_counter: StepCounter<'d, TC>,
    dir_pin: Output<'d>,
    dir: Dir,
    _disable: Output<'d>,

    angle_factor: f64,
}

impl<'d, T: GeneralInstance4Channel, TC: GeneralInstance4Channel, C: TimerChannel>
    Stepper<'d, T, TC, C>
{
    pub fn new(
        step_interface: StepInterface<'d, T, C>,
        step_counter: StepCounter<'d, TC>,
        mut dir_pin: Output<'d>,
        mut disable: Output<'d>,
        steps_per_rev: u32,
    ) -> Self {
        let angle_factor = steps_per_rev as f64 / (2. * consts::PI);
        let dir = Dir::Cw;
        disable.set_low();
        dir_pin.set_level(dir.level());
        Self {
            step_interface,
            step_counter,
            dir_pin,
            dir,
            _disable: disable,
            angle_factor,
        }
    }

    pub fn set_speed(&mut self, speed: f64) {
        let frequency = speed * self.angle_factor;
        self.step_interface
            .set_frequency(hz(frequency.abs() as u32));
        self.set_dir(frequency);
        self.step_interface.start();
    }

    pub fn get_speed(&mut self) -> f64 {
        if self.step_interface.is_enabled() {
            let frequency = self.step_interface.get_frequency().0;
            frequency as f64 / self.angle_factor * self.dir.float()
        } else {
            0.
        }
    }

    pub fn stop(&mut self) {
        self.step_interface.stop();
    }

    pub fn get_pos(&mut self) -> f64 {
        self.step_counter.get_pos() as f64 / self.angle_factor
    }

    fn set_dir(&mut self, dir: f64) {
        self.dir = Dir::from_float(dir);
        self.dir_pin.set_level(self.dir.level());
        self.step_counter.set_dir(self.dir);
    }
}
