#[derive(Clone)]
pub struct Spring {
    pub position: f32,
    pub velocity: f32,
}

impl Spring {
    #[inline]
    pub fn new() -> Self {
        Self {
            position: 0.0,
            velocity: 0.0,
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        self.position = 0.0;
        self.velocity = 0.0;
    }

    /// Advance by variable `dt`. Returns `true` while still moving.
    #[inline]
    pub fn update(&mut self, dt: f32, animation_length: f32) -> bool {
        if animation_length <= dt {
            self.reset();
            return false;
        }
        if self.position == 0.0 {
            return false;
        }

        // Critically-damped spring (zeta = 1.0).
        // omega chosen so destination is reached within ~2% tolerance in
        // `animation_length` time.
        let omega = 4.0 / animation_length;

        // Analytical solution for critically-damped harmonic oscillation.
        let a = self.position;
        let b = a * omega + self.velocity;
        let c = (-omega * dt).exp();

        self.position = (a + b * dt) * c;
        self.velocity = c * (-a * omega - b * dt * omega + b);

        if self.position.abs() < 0.01 {
            self.reset();
            false
        } else {
            true
        }
    }
}
