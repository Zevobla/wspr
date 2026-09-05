//! MD3 motion tokens: the duration scale and easing curves the Flow Bar's
//! state-transition animation (`crate::flow_bar`) is built from.
//!
//! MD3's easing/duration system is defined for *transitions* (entering,
//! exiting, staying on screen) -- see the material-3 skill's motion
//! reference -- so it covers the Flow Bar's one-shot "Done" fade-in and
//! the eased sweep of its "Thinking" progress indicator directly. The
//! continuous "Recording" pulse is a different kind of animation (a
//! looping breath, not a transition) and isn't part of this token system;
//! `crate::flow_bar` drives it with a plain sine oscillator instead, only
//! borrowing its period from this duration scale.

use std::time::Duration;

/// MD3 duration scale entry `medium4` -- paired with
/// `CubicBezier::EMPHASIZED_DECELERATE` for "element enters screen" per
/// the skill's suggested pairings table. Used as the Flow Bar's "Done"
/// fade-in length.
pub const MEDIUM_4: Duration = Duration::from_millis(400);
/// MD3 duration scale entry `long2`. Used as half of the Recording pulse's
/// full breath cycle.
pub const LONG_2: Duration = Duration::from_millis(500);
/// MD3 duration scale entry `extraLong2`. Used as the Thinking sweep's
/// cycle length.
pub const EXTRA_LONG_2: Duration = Duration::from_millis(800);

/// A cubic-bezier easing curve, evaluated the same way CSS
/// `cubic-bezier(x1, y1, x2, y2)` timing functions are: the curve's start
/// and end points are pinned at `(0, 0)` and `(1, 1)`, and `(x1, y1)`/
/// `(x2, y2)` are its two control points.
#[derive(Debug, Clone, Copy)]
pub struct CubicBezier {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

impl CubicBezier {
    /// MD3 "emphasized": begins and ends on screen. Used for the Flow
    /// Bar's "Thinking" sweep, which loops rather than entering/exiting.
    pub const EMPHASIZED: Self = Self::new(0.2, 0.0, 0.0, 1.0);
    /// MD3 "emphasized decelerate": entering the screen. Used for the Flow
    /// Bar's "Done" fade-in.
    pub const EMPHASIZED_DECELERATE: Self = Self::new(0.05, 0.7, 0.1, 1.0);

    const fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }

    /// Evaluates the curve at normalized time `t` (clamped to
    /// `0.0..=1.0`), returning the eased progress. Solves for the bezier
    /// parameter whose X matches `t` via Newton-Raphson, then evaluates Y
    /// at that parameter -- the same approach browsers use for CSS
    /// `cubic-bezier()` timing functions.
    pub fn ease(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let param = self.solve_for_x(t);
        self.sample(param, self.y1, self.y2)
    }

    fn sample(&self, t: f32, p1: f32, p2: f32) -> f32 {
        let mt = 1.0 - t;
        3.0 * mt * mt * t * p1 + 3.0 * mt * t * t * p2 + t * t * t
    }

    fn sample_dx(&self, t: f32) -> f32 {
        let mt = 1.0 - t;
        3.0 * mt * mt * self.x1 + 6.0 * mt * t * (self.x2 - self.x1) + 3.0 * t * t * (1.0 - self.x2)
    }

    fn solve_for_x(&self, x: f32) -> f32 {
        let mut t = x;
        for _ in 0..8 {
            let x_at_t = self.sample(t, self.x1, self.x2) - x;
            if x_at_t.abs() < 1e-4 {
                return t;
            }
            let derivative = self.sample_dx(t);
            if derivative.abs() < 1e-6 {
                break;
            }
            t -= x_at_t / derivative;
        }
        t.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_is_pinned_at_the_endpoints() {
        for curve in [CubicBezier::EMPHASIZED, CubicBezier::EMPHASIZED_DECELERATE] {
            assert!(curve.ease(0.0).abs() < 1e-3);
            assert!((curve.ease(1.0) - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn emphasized_decelerate_front_loads_progress() {
        // "Decelerate" curves move fast up front and slow down, so the
        // midpoint should land well past halfway.
        let curve = CubicBezier::EMPHASIZED_DECELERATE;
        assert!(curve.ease(0.5) > 0.5);
    }

    #[test]
    fn ease_clamps_out_of_range_input() {
        let curve = CubicBezier::EMPHASIZED;
        assert_eq!(curve.ease(-1.0), curve.ease(0.0));
        assert_eq!(curve.ease(2.0), curve.ease(1.0));
    }
}
