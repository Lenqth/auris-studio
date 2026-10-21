//! Continuous passive bow scattering with an independently controlled bow-speed ramp.

use super::super::Settings;

#[derive(Clone, Debug, Default)]
pub(super) struct Bow {
    velocity: f32,
    motion: f32,
    ramp: f32,
    rate: f32,
    speed: f32,
    hardness: f32,
}

impl Bow {
    pub(super) fn motion(&self) -> f32 {
        self.motion
    }
    pub(super) fn excite(&mut self, velocity: f32, rate: f32, settings: Settings) {
        self.velocity = velocity;
        self.motion = 0.0;
        self.rate = rate;
        self.update(settings);
    }

    pub(super) fn update(&mut self, settings: Settings) {
        self.ramp = (1.0 / (self.rate * settings.bow_response)).min(1.0);
        self.speed = settings.bow_speed;
        self.hardness = settings.hardness;
    }

    pub(super) fn set_velocity(&mut self, velocity: f32) {
        self.velocity = velocity;
    }

    pub(super) fn next(&mut self, incoming: f32, expression: f32, pressure: f32) -> f32 {
        let target = self.velocity * 0.12 * self.speed * expression;
        self.motion += (target - self.motion) * self.ramp;
        let difference = self.motion - incoming;
        let static_limit = self.velocity * (0.008 + 0.09 * pressure.clamp(0.0, 1.0));
        // A bow-table scattering coefficient joins sticking to sliding continuously.
        // A hard force switch excites short-segment ringing on every breakaway, especially
        // during bass attacks. Cubic/quartic tails retain the independent hardness control.
        let relative = difference.abs() / static_limit.max(1e-6);
        let grip = (0.75 + 0.25 * relative).max(1.0).recip();
        let reflection = grip.powi(3) * (self.hardness + (1.0 - self.hardness) * grip);
        // 0 <= reflection <= 1 preserves passivity relative to the moving bow.
        difference * reflection
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossing_the_slip_threshold_does_not_create_a_force_step() {
        for pressure in [0.2, 0.5, 0.8] {
            for hardness in [0.0, 0.5, 1.0] {
                let mut bow = Bow {
                    velocity: 0.8,
                    hardness,
                    ..Bow::default()
                };
                let threshold = 0.8 * (0.008 + 0.09 * pressure);
                let before = bow.next(-threshold + 1e-7, 0.0, pressure);
                let after = bow.next(-threshold - 1e-7, 0.0, pressure);
                assert!(
                    (after - before).abs() < 1e-5,
                    "force step: {before} -> {after}"
                );
            }
        }
    }

    #[test]
    fn friction_grips_at_low_speed_and_never_reverses_relative_slip() {
        let mut bow = Bow {
            velocity: 0.8,
            hardness: 0.7,
            ..Bow::default()
        };
        let force = bow.next(-0.2, 0.0, 0.5);
        assert!((0.0..0.2).contains(&force));
        assert_eq!(bow.next(-0.005, 0.0, 0.5), 0.005);
        for pressure in [0.0, 0.5, 1.0] {
            for incoming in [-1.0, -0.01, 0.0, 0.01, 1.0] {
                let force = bow.next(incoming, 0.0, pressure);
                assert!(force.abs() <= incoming.abs() + 1e-7);
                assert!(force * -incoming >= 0.0);
            }
        }
    }
}
