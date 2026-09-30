//! Snapping to the grid (REFERENCE-001 section 17).
use super::{Editor, Modifiers};
use crate::geometry::Point;

impl Editor {
    /// `getGridPoint`: `at` on the nearest grid point while the grid is on
    /// and Ctrl/Cmd is not held; otherwise `at` itself.
    pub(super) fn snap(&self, at: Point, modifiers: Modifiers) -> Point {
        match self.grid_size(modifiers) {
            Some(size) => at.map(|v| to_grid(v, size)),
            None => at,
        }
    }

    /// The grid size to snap to, if any.
    pub(super) fn grid_size(&self, modifiers: Modifiers) -> Option<f64> {
        if modifiers.command {
            return None;
        }
        self.scene.grid().map(|grid| grid.size)
    }
}

/// `value` on the nearest multiple of `size`, halves rounding up like
/// JavaScript's `Math.round` (also below zero).
pub(super) fn to_grid(value: f64, size: f64) -> f64 {
    (value / size + 0.5).floor() * size
}
