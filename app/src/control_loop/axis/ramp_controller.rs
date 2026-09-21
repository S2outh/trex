pub struct TrapezoidRampController {
    a_max: f64,
    v_lim: f64,
    v_max: f64,
    min_percent_max_speed: f64,
}

impl TrapezoidRampController {
    pub fn new(a_max: f64, v_max: f64, min_percent_max_speed: f64) -> Self {
        let v_lim = v_max;
        Self {
            a_max,
            v_lim,
            v_max,
            min_percent_max_speed,
        }
    }
    pub fn update(&mut self, v: f64, d: f64, dt: f64) -> f64 {
        let v_abs = v.abs();
        let d_abs = d.abs();

        // At low speeds, update v_max
        let halt_speed = dt * self.a_max;
        if v_abs < halt_speed {
            self.v_lim = (self.a_max
                * libm::sqrt(d_abs * (1. - self.min_percent_max_speed) / self.a_max))
            .min(self.v_max);
        }

        let dist_to_v0 = 0.5 * v_abs * v_abs / self.a_max;

        let a = if dist_to_v0 < d_abs {
            // accelerating ramp
            self.a_max * d.signum()
        } else {
            // decelerating ramp
            -self.a_max * v.signum()
        };
        (v + a * dt).clamp(-self.v_lim, self.v_lim)
    }
}
