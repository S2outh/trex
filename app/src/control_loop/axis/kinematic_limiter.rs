pub struct KinematicLimiter {
    lower_bound: f64,
    upper_bound: f64,
    v_max: f64,
    a_max: f64,
}

impl KinematicLimiter {
    pub fn new(lower_bound: f64, upper_bound: f64, v_max: f64, a_max: f64) -> Self {
        Self {
            lower_bound,
            upper_bound,
            v_max,
            a_max,
        }
    }
    fn get_limit_vel(&self, pos: f64, limit: f64) -> f64 {
        let limit_dist = limit - pos;
        libm::sqrt(2. * limit_dist.abs() * self.a_max) * limit_dist.signum()
    }
    pub fn clamp_vel(&self, pos: f64, vel: f64) -> f64 {
        let lower_limit_vel = self.get_limit_vel(pos, self.lower_bound).max(-self.v_max);
        let upper_limit_vel = self.get_limit_vel(pos, self.upper_bound).min(self.v_max);

        vel.clamp(lower_limit_vel, upper_limit_vel)
    }
}
