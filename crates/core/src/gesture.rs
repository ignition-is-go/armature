//! The arithmetic behind gestures: how fast a pointer is moving, where a flick will
//! land, how a spring settles, and how far to give when pulled past an edge.

/// Samples older than this are too stale to describe the current motion.
const WINDOW_MS: f64 = 100.0;

/// Measures velocity from recent positions along one axis.
#[derive(Clone, Debug, Default)]
pub struct VelocityTracker {
    samples: Vec<(f64, f64)>,
}

impl VelocityTracker {
    pub fn push(&mut self, time_ms: f64, position: f64) {
        self.samples.push((time_ms, position));
        self.samples.retain(|(time, _)| time_ms - time <= WINDOW_MS);
    }

    /// Units per second over the recent window. Zero without two samples apart in time.
    pub fn velocity(&self) -> f64 {
        match (self.samples.first(), self.samples.last()) {
            (Some((t0, p0)), Some((t1, p1))) if t1 > t0 => (p1 - p0) / (t1 - t0) * 1000.0,
            _ => 0.0,
        }
    }
}

/// How much velocity survives each millisecond of a coasting flick.
pub const DECELERATION: f64 = 0.998;

/// How far a value moving at `velocity` (units per second) travels before friction
/// stops it.
pub fn project_distance(velocity: f64, deceleration: f64) -> f64 {
    velocity / 1000.0 * deceleration / (1.0 - deceleration)
}

/// Resistance past an edge: the further the pull, the less the value gives, never
/// exceeding `dimension`.
pub fn rubber_band(overshoot: f64, dimension: f64, stiffness: f64) -> f64 {
    if dimension <= 0.0 {
        return 0.0;
    }
    let sign = overshoot.signum();
    let pulled = overshoot.abs() * stiffness / dimension;
    sign * (1.0 - 1.0 / (pulled + 1.0)) * dimension
}

/// Which of `sectors` equal slices a stroke points into, for something like a radial
/// menu. Slice 0 is centred on straight up and the count goes clockwise. `dy` grows
/// downwards, as on screen.
pub fn sector_of(dx: f64, dy: f64, sectors: usize) -> usize {
    let sectors = sectors.max(1);
    let clockwise_from_up = dx.atan2(-dy).rem_euclid(std::f64::consts::TAU);
    let slice = std::f64::consts::TAU / sectors as f64;
    ((clockwise_from_up + slice / 2.0) / slice) as usize % sectors
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub stiffness: f64,
    pub damping: f64,
    pub mass: f64,
}

impl Spring {
    /// Position and velocity `seconds` after release at `from` with `velocity`, heading
    /// for `to`. Solved exactly, so any time can be asked for directly.
    pub fn solve(&self, from: f64, to: f64, velocity: f64, seconds: f64) -> (f64, f64) {
        let offset = from - to;
        let natural = (self.stiffness / self.mass).sqrt();
        let ratio = self.damping / (2.0 * (self.stiffness * self.mass).sqrt());
        let t = seconds;

        let (position, speed) = if (ratio - 1.0).abs() < 1e-6 {
            let b = velocity + natural * offset;
            let decay = (-natural * t).exp();
            (
                decay * (offset + b * t),
                decay * (b - natural * (offset + b * t)),
            )
        } else if ratio < 1.0 {
            let damped = natural * (1.0 - ratio * ratio).sqrt();
            let b = (velocity + ratio * natural * offset) / damped;
            let decay = (-ratio * natural * t).exp();
            let (sin, cos) = (damped * t).sin_cos();
            (
                decay * (offset * cos + b * sin),
                decay
                    * ((b * damped - ratio * natural * offset) * cos
                        - (offset * damped + ratio * natural * b) * sin),
            )
        } else {
            let spread = (ratio * ratio - 1.0).sqrt();
            let slow = -natural * (ratio - spread);
            let fast = -natural * (ratio + spread);
            let c1 = (velocity - fast * offset) / (slow - fast);
            let c2 = offset - c1;
            let (e1, e2) = ((slow * t).exp(), (fast * t).exp());
            (c1 * e1 + c2 * e2, c1 * slow * e1 + c2 * fast * e2)
        };
        (to + position, speed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn velocity_comes_from_the_recent_window_only() {
        let mut tracker = VelocityTracker::default();
        tracker.push(0.0, 0.0);
        tracker.push(500.0, 1000.0);
        tracker.push(550.0, 1010.0);
        assert!(
            close(tracker.velocity(), 200.0),
            "the old fast stretch is forgotten"
        );
    }

    #[test]
    fn a_single_sample_has_no_velocity() {
        let mut tracker = VelocityTracker::default();
        tracker.push(0.0, 5.0);
        assert_eq!(tracker.velocity(), 0.0);
    }

    #[test]
    fn a_faster_flick_travels_proportionally_further() {
        let slow = project_distance(1000.0, DECELERATION);
        assert!(close(slow, 499.0));
        assert!(close(project_distance(-2000.0, DECELERATION), -2.0 * slow));
    }

    #[test]
    fn rubber_banding_gives_less_the_further_it_is_pulled() {
        let near = rubber_band(50.0, 400.0, 0.55);
        let far = rubber_band(500.0, 400.0, 0.55);
        assert!(near < 50.0 && far < 400.0);
        assert!(far - near < 450.0 * 0.55);
        assert!(close(rubber_band(-50.0, 400.0, 0.55), -near));
    }

    #[test]
    fn sectors_count_clockwise_from_straight_up() {
        assert_eq!(sector_of(0.0, -10.0, 4), 0);
        assert_eq!(sector_of(10.0, 0.0, 4), 1);
        assert_eq!(sector_of(0.0, 10.0, 4), 2);
        assert_eq!(sector_of(-10.0, 0.0, 4), 3);
        assert_eq!(
            sector_of(-1.0, -10.0, 4),
            0,
            "slice 0 straddles straight up"
        );
    }

    #[test]
    fn a_spring_starts_where_released_and_settles_on_the_target() {
        for damping in [10.0, 20.0, 60.0] {
            let spring = Spring {
                stiffness: 100.0,
                damping,
                mass: 1.0,
            };
            let (start, speed) = spring.solve(0.0, 100.0, 30.0, 0.0);
            assert!(close(start, 0.0) && close(speed, 30.0));
            let (end, rest) = spring.solve(0.0, 100.0, 30.0, 20.0);
            assert!((end - 100.0).abs() < 1e-3 && rest.abs() < 1e-3);
        }
    }

    #[test]
    fn only_an_underdamped_spring_overshoots() {
        let peak = |damping: f64| {
            let spring = Spring {
                stiffness: 100.0,
                damping,
                mass: 1.0,
            };
            (0..400)
                .map(|i| spring.solve(0.0, 100.0, 0.0, i as f64 * 0.01).0)
                .fold(0.0, f64::max)
        };
        assert!(peak(5.0) > 100.5);
        assert!(peak(20.0) <= 100.0 + 1e-9, "critically damped");
        assert!(peak(60.0) <= 100.0 + 1e-9, "overdamped");
    }

    #[test]
    fn the_solved_velocity_matches_the_slope_of_the_position() {
        for damping in [5.0, 20.0, 60.0] {
            let spring = Spring {
                stiffness: 100.0,
                damping,
                mass: 1.0,
            };
            let h = 1e-5;
            let (before, _) = spring.solve(0.0, 100.0, 40.0, 0.3 - h);
            let (after, _) = spring.solve(0.0, 100.0, 40.0, 0.3 + h);
            let (_, speed) = spring.solve(0.0, 100.0, 40.0, 0.3);
            assert!((speed - (after - before) / (2.0 * h)).abs() < 1e-3);
        }
    }
}
