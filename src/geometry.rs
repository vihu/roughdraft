//! Points and affine transforms.

/// A point or vector, in scene or element-local units. Y points down.
pub type Point = [f64; 2];

/// A 2D affine transform, laid out like SVG's `matrix(a b c d e f)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Affine {
    /// Leaves points unchanged.
    pub const IDENTITY: Self = Self::scale(1.0);

    /// Moves points by `offset`.
    pub const fn translate([x, y]: Point) -> Self {
        Self {
            e: x,
            f: y,
            ..Self::IDENTITY
        }
    }

    /// Scales uniformly around the origin.
    pub const fn scale(factor: f64) -> Self {
        Self {
            a: factor,
            b: 0.0,
            c: 0.0,
            d: factor,
            e: 0.0,
            f: 0.0,
        }
    }

    /// Rotates by `angle` radians around `center`, clockwise on screen (like
    /// SVG's `rotate(deg cx cy)`).
    pub fn rotate_about(angle: f64, [cx, cy]: Point) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: cx - cos * cx + sin * cy,
            f: cy - sin * cx - cos * cy,
        }
    }

    /// Returns the transform that applies `self`, then `next`.
    #[must_use]
    pub fn then(self, next: Self) -> Self {
        Self {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            e: next.a * self.e + next.c * self.f + next.e,
            f: next.b * self.e + next.d * self.f + next.f,
        }
    }

    /// Transforms a point.
    #[inline]
    pub fn apply(&self, [x, y]: Point) -> Point {
        [
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        ]
    }

    /// Returns how much lengths grow, assuming no skew or uneven scaling.
    pub fn scale_factor(&self) -> f64 {
        (self.a * self.d - self.b * self.c).abs().sqrt()
    }

    /// Returns the rotation in radians.
    pub fn rotation(&self) -> f64 {
        self.b.atan2(self.a)
    }
}

#[cfg(test)]
mod tests {
    use super::{Affine, Point};
    use std::f64::consts::FRAC_PI_2;

    fn assert_close(got: Point, want: Point) {
        assert!(
            (got[0] - want[0]).abs() < 1e-12 && (got[1] - want[1]).abs() < 1e-12,
            "{got:?} != {want:?}"
        );
    }

    #[test]
    fn rotate_about_center_is_clockwise_on_screen() {
        let quarter = Affine::rotate_about(FRAC_PI_2, [1.0, 1.0]);
        assert_close(quarter.apply([2.0, 1.0]), [1.0, 2.0]);
        assert_close(quarter.apply([1.0, 1.0]), [1.0, 1.0]);
    }

    #[test]
    fn then_applies_in_order() {
        let t = Affine::translate([10.0, 0.0]).then(Affine::scale(2.0));
        assert_close(t.apply([1.0, 1.0]), [22.0, 2.0]);
        assert_eq!(t.scale_factor(), 2.0);
        let r = Affine::rotate_about(FRAC_PI_2, [0.0, 0.0]).then(Affine::scale(3.0));
        assert!((r.rotation() - FRAC_PI_2).abs() < 1e-12);
        assert!((r.scale_factor() - 3.0).abs() < 1e-12);
    }
}
