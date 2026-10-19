pub struct TrapezoidRampController {
    a_max: f64,
    v_max: f64,
}

impl TrapezoidRampController {
    pub fn new(a_max: f64, v_max: f64) -> Self {
        Self { a_max, v_max }
    }
    pub fn update(&mut self, v_rel: f64, dist: f64, dt: f64) -> f64 {
        let v_rel_abs = v_rel.abs();
        let dist_abs = dist.abs();

        let dist_to_v0 = 0.5 * v_rel_abs * v_rel_abs / self.a_max;

        let a = if dist_to_v0 < dist_abs {
            // accelerating ramp
            self.a_max * dist.signum()
        } else {
            // decelerating ramp
            -self.a_max * v_rel.signum()
        };

        (v_rel + a * dt).clamp(-self.v_max, self.v_max)
    }
}
