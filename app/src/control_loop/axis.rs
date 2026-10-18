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
const ANGLE_LIMITS: f64 = core::f64::consts::PI;

impl<'d, T, TC, C> Axis<'d, T, TC, C>
where
    T: GeneralInstance4Channel,
    TC: GeneralInstance4Channel,
    C: TimerChannel,
{
    pub fn new(stepper: Stepper<'d, T, TC, C>, max_speed: f64, acceleration: f64) -> Self {
        let controller = TrapezoidRampController::new(acceleration, max_speed);
        Self {
            stepper,
            controller,
            acceleration,
        }
    }

    pub fn update(&mut self, mut target_pos: f64, target_vel: f64, dt: f64) -> AxisState {
        target_pos = target_pos.clamp(-ANGLE_LIMITS, ANGLE_LIMITS);

        let current_pos = self.stepper.get_pos();
        let current_vel = self.stepper.get_speed();
        let pos_diff = target_pos - current_pos;
        let relative_vel = current_vel - target_vel;

        // calculate velocity deadband from acceleration to prevent overshoot
        let velocity_db = dt * self.acceleration;
        if pos_diff.abs() < DEADBAND && relative_vel.abs() < velocity_db {
            if current_vel < velocity_db {
                self.stepper.stop();
            } else {
                self.stepper.set_speed(target_vel);
            }
            return AxisState {
                pos: current_pos,
                vel: current_vel,
            };
        }

        let new_vel = target_vel + self.controller.update(relative_vel, pos_diff, dt);

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
