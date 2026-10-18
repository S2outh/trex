pub struct TrapezoidRampController {
    a_max: f64,
    v_max: f64,
}

impl TrapezoidRampController {
    pub fn new(a_max: f64, v_max: f64) -> Self {
        Self { a_max, v_max }
    }
    pub fn update(&mut self, v: f64, d: f64, dt: f64) -> f64 {
        let v_abs = v.abs();
        let d_abs = d.abs();

        let dist_to_v0 = 0.5 * v_abs * v_abs / self.a_max;

        let a = if dist_to_v0 < d_abs {
            // accelerating ramp
            self.a_max * d.signum()
        } else {
            // decelerating ramp
            -self.a_max * v.signum()
        };

        (v + a * dt).clamp(-self.v_max, self.v_max)
    }
}
