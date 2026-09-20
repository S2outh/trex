use embassy_stm32::timer::{GeneralInstance4Channel, TimerChannel};

mod ramp_controller;

use crate::{control_loop::axis::ramp_controller::TrapezoidRampController, drivers::stepper::Stepper};

pub struct Axis<'d, T, TC, C>
where
    T: GeneralInstance4Channel,
    TC: GeneralInstance4Channel,
    C: TimerChannel,
{
    stepper: Stepper<'d, T, TC, C>,
    controller: TrapezoidRampController,
}

// limits
const DEADBAND: f64 = 0.05; // rad
const HALT_SPEED: f64 = 0.03; // rad/s
const MAX_SPEED: f64 = 0.6; // rad/s
const ACCELERATION: f64 = 0.5; // rad/s^2

// min 50 percent of the travel should be spent at max speed,
// the controller uses this value to dynamically recalculate a
// max speed at runtime
const MIN_PERCENT_MAX_SPEED: f64 = 0.5;

impl<'d, T, TC, C> Axis<'d, T, TC, C>
where
    T: GeneralInstance4Channel,
    TC: GeneralInstance4Channel,
    C: TimerChannel,
{
    pub fn new(stepper: Stepper<'d, T, TC, C>) -> Self {
        let controller = TrapezoidRampController::new(ACCELERATION, MAX_SPEED, MIN_PERCENT_MAX_SPEED);
        Self { stepper, controller }
    }

    pub fn set_target_pos(&mut self, target_pos: f64, dt: f64) {
        let current_pos = self.stepper.get_pos();
        let current_speed = self.stepper.get_speed();
        let pos_diff = target_pos - current_pos;

        if pos_diff.abs() < DEADBAND && current_speed.abs() < HALT_SPEED {
            self.stepper.stop();
            return;
        }

        let new_speed = self.controller.update(current_speed, pos_diff, dt);

        self.stepper.set_speed(new_speed);
    }

    pub fn stop(&mut self) {
        self.stepper.stop();
    }

    pub fn get_pos(&mut self) -> f64 {
        self.stepper.get_pos()
    }
}
