//! Points, affine transforms and element bounds.
use crate::scene::{Element, Kind};

/// A point or vector, in scene or element-local units. Y points down.
pub type Point = [f64; 2];

/// Axis-aligned box `[min_x, min_y, max_x, max_y]`.
pub type Bounds = [f64; 4];

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

    /// Returns the transform that undoes this one.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if the transform collapses space (zero scale).
    #[must_use]
    pub fn inverse(&self) -> Self {
        let det = self.a * self.d - self.b * self.c;
        debug_assert!(det != 0.0, "transform is not invertible");
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Self {
            a,
            b,
            c,
            d,
            e: -(a * self.e + c * self.f),
            f: -(b * self.e + d * self.f),
        }
    }
}

/// Returns an element's local box: `[0, 0, width, height]`, or the points'
/// box for lines and arrows (their points can go negative).
pub fn local_bounds(element: &Element) -> Bounds {
    match &element.kind {
        Kind::Line(line) | Kind::Arrow(line) if !line.points.is_empty() => line.points.iter().fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |[x1, y1, x2, y2], [x, y]| [x1.min(*x), y1.min(*y), x2.max(*x), y2.max(*y)],
        ),
        _ => [0.0, 0.0, element.base.width, element.base.height],
    }
}

/// Maps element-local points to scene points: rotation around the local
/// box center, then translation to `x`/`y`.
// ponytail: rotates lines around their points' center, Excalidraw uses the rough curve's (render.rs); differs only for rotated curves
pub fn element_transform(element: &Element) -> Affine {
    let [x1, y1, x2, y2] = local_bounds(element);
    let center = [(x1 + x2) / 2.0, (y1 + y2) / 2.0];
    Affine::rotate_about(element.base.angle, center)
        .then(Affine::translate([element.base.x, element.base.y]))
}

/// Returns the scene-space box around an element after rotation.
pub fn element_bounds(element: &Element) -> Bounds {
    let [x1, y1, x2, y2] = local_bounds(element);
    let transform = element_transform(element);
    [[x1, y1], [x2, y1], [x2, y2], [x1, y2]]
        .map(|corner| transform.apply(corner))
        .iter()
        .fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |[a, b, c, d], [x, y]| [a.min(*x), b.min(*y), c.max(*x), d.max(*y)],
        )
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
    fn inverse_undoes() {
        let t = Affine::rotate_about(0.7, [3.0, 4.0])
            .then(Affine::translate([10.0, -2.0]))
            .then(Affine::scale(1.5));
        assert_close(t.inverse().apply(t.apply([5.0, 6.0])), [5.0, 6.0]);
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
