use embassy_stm32::timer::{GeneralInstance4Channel, TimerChannel};

mod ramp_controller;

use crate::{
    control_loop::axis::ramp_controller::TrapezoidRampController, drivers::stepper::Stepper,
};

pub struct AxisState {
    pub pos: f64,
    pub vel: f64,
}

pub struct Axis<'d, T, TC, C>
where
    T: GeneralInstance4Channel,
    TC: GeneralInstance4Channel,
    C: TimerChannel,
{
    stepper: Stepper<'d, T, TC, C>,
    controller: TrapezoidRampController,
    acceleration: f64,
}

// limits
const DEADBAND: f64 = 0.05; // rad

// min 50 percent of the travel should be spent at the speed limit,
// the controller uses this value to dynamically recalculate a
// max speed at runtime.
const MIN_PERCENT_MAX_SPEED: f64 = 0.5;

impl<'d, T, TC, C> Axis<'d, T, TC, C>
where
    T: GeneralInstance4Channel,
    TC: GeneralInstance4Channel,
    C: TimerChannel,
{
    pub fn new(stepper: Stepper<'d, T, TC, C>, max_speed: f64, acceleration: f64) -> Self {
        let controller =
            TrapezoidRampController::new(acceleration, max_speed, MIN_PERCENT_MAX_SPEED);
        Self {
            stepper,
            controller,
            acceleration,
        }
    }

    pub fn update(&mut self, target_pos: f64, dt: f64) -> AxisState {
        let current_pos = self.stepper.get_pos();
        let current_vel = self.stepper.get_speed();
        let pos_diff = target_pos - current_pos;

        // calculate halt_speed from acceleration to prevent overshoot
        let halt_speed = dt * self.acceleration;
        if pos_diff.abs() < DEADBAND && current_vel.abs() < halt_speed {
            self.stepper.stop();
            return AxisState {
                pos: current_pos,
                vel: 0.,
            };
        }

        let new_vel = self.controller.update(current_vel, pos_diff, dt);

        self.stepper.set_speed(new_vel);

        AxisState {
            pos: current_pos,
            vel: new_vel,
        }
    }

    pub fn stop(&mut self) {
        self.stepper.stop();
    }

    pub fn get_pos(&mut self) -> f64 {
        self.stepper.get_pos()
    }
}
