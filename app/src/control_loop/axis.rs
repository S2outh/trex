use embassy_stm32::timer::{GeneralInstance4Channel, TimerChannel};

use crate::drivers::stepper::Stepper;

pub struct Axis<'d, T, TC, C>
where
    T: GeneralInstance4Channel,
    TC: GeneralInstance4Channel,
    C: TimerChannel,
{
    stepper: Stepper<'d, T, TC, C>,
}

impl<'d, T, TC, C>
    Axis<'d, T, TC, C>
where
    T: GeneralInstance4Channel,
    TC: GeneralInstance4Channel,
    C: TimerChannel,
{
    pub fn new(stepper: Stepper<'d, T, TC, C>) -> Self {
        Self { stepper }
    }

    pub fn set_speed(&mut self, speed: f64) {
        self.stepper.set_speed(speed);
    }

    pub fn stop(&mut self) {
        self.stepper.stop();
    }

    pub fn get_pos(&mut self) -> f64 {
        self.stepper.get_pos()
    }
}
