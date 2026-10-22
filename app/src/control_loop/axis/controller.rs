#[derive(Debug, Clone, Copy)]
pub struct PidParams {
    pub kp: f64,                // Proportional gain
    pub ki: Option<(f64, f64)>, // Integral gain and windup limit
    pub kd: Option<f64>,        // Derivative gain
    pub deadband: f64,          // error deadband (no action within this range)
}

pub struct PID {
    config: PidParams,
    integrated_error: f64,
    previous_error: f64,
}

impl PID {
    pub fn new(config: PidParams) -> Self {
        Self {
            config,
            integrated_error: 0.,
            previous_error: 0.,
        }
    }
    pub fn update(&mut self, error: f64, dt: f64) -> f64 {
        // remove deadband
        let effective_error = if error.abs() > self.config.deadband {
            error - error.signum() * self.config.deadband
        } else {
            0.
        };

        // proportional component
        let mut result = self.config.kp * effective_error;

        // integral component
        if let Some((ki, max_integral)) = self.config.ki {
            self.integrated_error += effective_error * dt;
            self.integrated_error = self.integrated_error.clamp(-max_integral, max_integral);

            result += ki * self.integrated_error;
        }

        // derivative component
        if let Some(kd) = self.config.kd {
            let derivative_error = if dt > 0. {
                (effective_error - self.previous_error) / dt
            } else {
                0.
            };
            self.previous_error = effective_error;

            result += kd * derivative_error;
        }

        result
    }
}

pub struct StagedPID {
    a_max: f64,
    stage_1: PID,
    stage_2: PID,
}

impl StagedPID {
    pub fn new(a_max: f64, config_1: PidParams, config_2: PidParams) -> Self {
        let stage_1 = PID::new(config_1);
        let stage_2 = PID::new(config_2);
        Self {
            a_max,
            stage_1,
            stage_2,
        }
    }
    pub fn update(&mut self, pos_error: f64, current_vel: f64, target_vel: f64, dt: f64) -> f64 {
        let v = self.stage_1.update(pos_error, dt) + target_vel;
        let a = self.stage_2.update(v - current_vel, dt);
        current_vel + a.clamp(-self.a_max, self.a_max) * dt
    }
}
