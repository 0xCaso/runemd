//! CSS-style `cubic-bezier(x1, y1, x2, y2)` easing, for the curves Animation Lab tunes.

/// The closest standard bezier to today's ease-out cubic, `1 - (1 - x)^3`.
pub const EASE_OUT_CUBIC: [f32; 4] = [0.33, 1.0, 0.68, 1.0];
/// Linear: the ring's turn today.
pub const LINEAR: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

/// The eased value of `x` (0..1) on the curve `p`. The two defaults keep today's exact formulas
/// (`EASE_OUT_CUBIC` the cubic, `LINEAR` the identity), so without the lab nothing moves by a bit.
pub fn ease(p: [f32; 4], x: f32) -> f32 {
    if p == EASE_OUT_CUBIC {
        1.0 - (1.0 - x).powi(3)
    } else if p == LINEAR {
        x
    } else {
        cubic_bezier(p, x)
    }
}

/// The y of the curve with control points `p = [x1, y1, x2, y2]` (from (0,0) to (1,1)) at `x`,
/// like CSS `cubic-bezier`: solves x(s) = x (Newton, then bisection), returns y(s).
pub fn cubic_bezier(p: [f32; 4], x: f32) -> f32 {
    let [x1, y1, x2, y2] = p;
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let at = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    };
    let slope = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * a + 6.0 * u * s * (b - a) + 3.0 * s * s * (1.0 - b)
    };
    let mut s = x;
    for _ in 0..8 {
        let err = at(x1, x2, s) - x;
        if err.abs() < 1e-6 {
            return at(y1, y2, s);
        }
        let d = slope(x1, x2, s);
        if d.abs() < 1e-6 {
            break;
        }
        s = (s - err / d).clamp(0.0, 1.0);
    }
    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    s = x;
    for _ in 0..32 {
        let v = at(x1, x2, s);
        if (v - x).abs() < 1e-6 {
            break;
        }
        if v < x {
            lo = s;
        } else {
            hi = s;
        }
        s = (lo + hi) / 2.0;
    }
    at(y1, y2, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_is_identity() {
        for i in 0..=20 {
            let x = i as f32 / 20.0;
            assert!((cubic_bezier([0.0, 0.0, 1.0, 1.0], x) - x).abs() < 1e-4, "{x}");
        }
    }

    #[test]
    fn ease_out_cubic_bezier_is_close_to_the_cubic() {
        let y = cubic_bezier([0.33, 1.0, 0.68, 1.0], 0.5);
        assert!((y - 0.875).abs() < 0.03, "{y}");
    }

    /// The defaults keep today's formulas exactly.
    #[test]
    fn the_defaults_are_todays_formulas() {
        for i in 0..=20 {
            let x = i as f32 / 20.0;
            assert_eq!(ease(EASE_OUT_CUBIC, x), 1.0 - (1.0 - x).powi(3));
            assert_eq!(ease(LINEAR, x), x);
        }
    }

    #[test]
    fn a_tuned_curve_is_the_bezier() {
        let p = [0.42, 0.0, 0.58, 1.0];
        assert_eq!(ease(p, 0.3), cubic_bezier(p, 0.3));
        assert!((cubic_bezier(p, 0.5) - 0.5).abs() < 1e-3, "ease-in-out is symmetric");
        assert!(cubic_bezier(p, 0.25) < 0.25, "slow start");
    }

    #[test]
    fn ends_are_fixed() {
        for p in [[0.33, 1.0, 0.68, 1.0], [0.0, 0.0, 1.0, 1.0], [0.7, -0.5, 0.2, 1.6]] {
            assert_eq!(cubic_bezier(p, 0.0), 0.0);
            assert_eq!(cubic_bezier(p, 1.0), 1.0);
        }
    }
}
