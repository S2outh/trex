pub struct TrapezoidRampController {
    a: f64,
    v_lim: f64,
    v_max: f64,
    min_percent_max_speed: f64,
}

impl TrapezoidRampController {
    pub fn new(a: f64, v_max: f64, min_percent_max_speed: f64) -> Self {
        let v_lim = v_max;
        Self { a, v_lim, v_max, min_percent_max_speed }
    }
    pub fn update(&mut self, v: f64, d: f64, dt: f64) -> f64 {
        let v_abs = v.abs();
        let d_abs = d.abs();

        // At low speeds, update v_max
        if v_abs < 0.05 {
            self.v_lim = (self.a * libm::sqrt(d_abs * (1. - self.min_percent_max_speed) / self.a)).min(self.v_max);
        }

        let dts = 0.5 * v_abs * v_abs / self.a;

        let accel = if dts < d_abs {
            self.a * dt * d.signum()
        } else {
            - self.a * dt * v.signum()
        };
        (v + accel).clamp(-self.v_lim, self.v_lim)
    }
}
