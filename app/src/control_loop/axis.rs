use embassy_stm32::timer::{GeneralInstance4Channel, TimerChannel};

mod ramp_controller;
mod kinematic_limiter;

use crate::{
    control_loop::axis::{kinematic_limiter::KinematicLimiter, ramp_controller::TrapezoidRampController}, drivers::stepper::Stepper,
};

pub struct AxisConfig {
    pub max_speed: f64,
    pub acceleration: f64,
    pub lower_bound: f64,
    pub upper_bound: f64,
}

pub struct Axis<'d, T, TC, C>
where
    T: GeneralInstance4Channel,
    TC: GeneralInstance4Channel,
    C: TimerChannel,
{
    stepper: Stepper<'d, T, TC, C>,
    controller: TrapezoidRampController,
    limiter: KinematicLimiter,
    acceleration: f64,
}

// limits
const DEADBAND: f64 = 0.05; // rad

impl<'d, T, TC, C> Axis<'d, T, TC, C>
where
    T: GeneralInstance4Channel,
    TC: GeneralInstance4Channel,
    C: TimerChannel,
{
    pub fn new(stepper: Stepper<'d, T, TC, C>, config: AxisConfig) -> Self {
        let controller = TrapezoidRampController::new(config.acceleration, config.max_speed);
        let limiter = KinematicLimiter::new(config.lower_bound, config.upper_bound, config.acceleration);
        Self {
            stepper,
            controller,
            limiter,
            acceleration: config.acceleration,
        }
    }

    pub fn update(&mut self, target_pos: f64, target_vel: f64, dt: f64) {
        let current_pos = self.stepper.get_pos();
        let current_vel = self.stepper.get_vel();
        let relative_pos = target_pos - current_pos;
        let relative_vel = current_vel - target_vel;

        // calculate velocity deadband from acceleration to prevent overshoot
        let velocity_db = dt * self.acceleration;
        if relative_pos.abs() < DEADBAND && relative_vel.abs() < velocity_db {
            if current_vel < velocity_db {
                self.stepper.stop();
            } else {
                self.stepper.set_vel(target_vel);
            }
            return;
        }

        let optimal_vel = target_vel + self.controller.update(relative_vel, relative_pos, dt);
        let limited_vel = self.limiter.clamp_vel(target_pos, optimal_vel);

        self.stepper.set_vel(limited_vel);
    }

    pub fn stop(&mut self) {
        self.stepper.stop();
    }

    pub fn get_pos(&mut self) -> f64 {
        self.stepper.get_pos()
    }

    pub fn get_vel(&mut self) -> f64 {
        self.stepper.get_vel()
    }
}
