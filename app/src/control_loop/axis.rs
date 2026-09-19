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

// limits
const DEADBAND: f64 = 0.05; // rad
const HALT_SPEED: f64 = 0.03; // rad/s
const MAX_SPEED: f64 = 0.6; // rad/s
const MAX_ACCELERATION: f64 = 0.5; // rad/s^2

// p values
const V_K_P: f64 = 0.5;
const A_K_P: f64 = 1.;

impl<'d, T, TC, C> Axis<'d, T, TC, C>
where
    T: GeneralInstance4Channel,
    TC: GeneralInstance4Channel,
    C: TimerChannel,
{
    pub fn new(stepper: Stepper<'d, T, TC, C>) -> Self {
        Self { stepper }
    }

    pub fn set_target_pos(&mut self, target_pos: f64, dt: f64) {
        let current_pos = self.stepper.get_pos();
        let current_speed = self.stepper.get_speed();
        let pos_diff = target_pos - current_pos;

        if pos_diff.abs() < DEADBAND && current_speed.abs() < HALT_SPEED {
            self.stepper.stop();
            return;
        }

        let v_p = pos_diff * V_K_P;
        let v_p_lim = v_p.clamp_magnitude(MAX_SPEED);

        let speed_diff = v_p_lim - current_speed;
        let a_p = speed_diff * A_K_P;
        let a_p_lim = a_p.clamp_magnitude(MAX_ACCELERATION);

        let new_speed = current_speed + a_p_lim * dt;

        self.stepper.set_speed(new_speed);
    }

    pub fn stop(&mut self) {
        self.stepper.stop();
    }

    pub fn get_pos(&mut self) -> f64 {
        self.stepper.get_pos()
    }
}
