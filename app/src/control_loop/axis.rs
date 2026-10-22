use embassy_stm32::timer::{GeneralInstance4Channel, TimerChannel};

pub mod controller;
mod kinematic_limiter;

use crate::{
    control_loop::axis::{
        controller::{PidParams, StagedPID},
        kinematic_limiter::KinematicLimiter,
    },
    drivers::stepper::Stepper,
};

pub struct AxisConfig {
    pub max_speed: f64,
    pub acceleration: f64,
    pub lower_bound: f64,
    pub upper_bound: f64,
    pub v_pid_params: PidParams,
    pub a_pid_params: PidParams,
}

pub struct Axis<'d, T, TC, C>
where
    T: GeneralInstance4Channel,
    TC: GeneralInstance4Channel,
    C: TimerChannel,
{
    stepper: Stepper<'d, T, TC, C>,
    controller: StagedPID,
    limiter: KinematicLimiter,
}

impl<'d, T, TC, C> Axis<'d, T, TC, C>
where
    T: GeneralInstance4Channel,
    TC: GeneralInstance4Channel,
    C: TimerChannel,
{
    pub fn new(stepper: Stepper<'d, T, TC, C>, config: AxisConfig) -> Self {
        let controller = StagedPID::new(
            config.acceleration,
            config.v_pid_params,
            config.a_pid_params,
        );
        let limiter = KinematicLimiter::new(
            config.lower_bound,
            config.upper_bound,
            config.max_speed,
            config.acceleration,
        );
        Self {
            stepper,
            controller,
            limiter,
        }
    }

    pub fn update(&mut self, target_pos: f64, target_vel: f64, dt: f64) {
        let current_pos = self.stepper.get_pos();
        let current_vel = self.stepper.get_vel();
        let relative_pos = target_pos - current_pos;

        let optimal_vel = self
            .controller
            .update(relative_pos, current_vel, target_vel, dt);
        let limited_vel = self.limiter.clamp_vel(current_pos, optimal_vel);

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
