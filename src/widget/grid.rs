//! The canvas grid, like Excalidraw's `strokeGrid` (REFERENCE-001 section
//! 17): a line every grid cell, dashed, and a solid one every `step` cells,
//! over the background and under the elements.
use iced::widget::canvas::Frame;
use iced::{Point, Size};

use super::Sketch;
use crate::color::Rgba;
use crate::scene::Grid;

/// `GridLineColor.Regular`, `#e5e5e5`.
const REGULAR: Rgba = Rgba::rgb(229.0 / 255.0, 229.0 / 255.0, 229.0 / 255.0);

/// `GridLineColor.Bold`, `#dddddd`.
const BOLD: Rgba = Rgba::rgb(221.0 / 255.0, 221.0 / 255.0, 221.0 / 255.0);

/// Screen pixels per cell below which only the bold lines are drawn.
const MIN_CELL: f64 = 10.0;

// Private API
impl Sketch {
    /// Draws the grid across `frame`, in screen pixels.
    pub(super) fn draw_grid(&self, frame: &mut Frame, grid: Grid) {
        let camera = self.camera.get();
        let zoom = camera.zoom;
        let size = frame.size();
        let extent = [f64::from(size.width), f64::from(size.height)];
        let cell = grid.size;
        let step = i64::from(grid.step);
        // One screen pixel wide, thinner when zoomed out (`min(1 / zoom,
        // bold ? 4 : 1)` scene units).
        let thin = zoom.min(1.0);
        let thick = (4.0 * zoom).min(1.0);
        // Dashes `[3w, w + 2 / zoom]` scene units, in screen pixels.
        let (dash, gap) = (3.0 * thin, thin + 2.0);
        // Crisp 1 px lines at 100%, like Excalidraw's half-pixel shift.
        let crisp = |at: f64| {
            if zoom == 1.0 && at.fract() == 0.0 {
                at + 0.5
            } else {
                at
            }
        };
        // Lines and dashes as thin rectangles: iced fills a rectangle
        // without tessellating a path, so a screen of dashes stays cheap.
        let (regular, bold) = (self.paint(REGULAR), self.paint(BOLD));
        for axis in 0..2 {
            // A bar `length` long from `along`, `width` thick, centred on
            // the line at `at`.
            let mut bar = |along: f64, at: f64, length: f64, width: f64, color| {
                let (x, y, w, h) = if axis == 0 {
                    (at - width / 2.0, along, width, length)
                } else {
                    (along, at - width / 2.0, length, width)
                };
                frame.fill_rectangle(
                    Point::new(x as f32, y as f32),
                    Size::new(w as f32, h as f32),
                    color,
                );
            };
            let origin = camera.origin[axis];
            let first = (origin / cell).floor() as i64;
            let last = ((origin + extent[axis] / zoom) / cell).ceil() as i64;
            // Along its length a line starts on the grid a cell before the
            // view, so its dashes move with the scene, not the screen.
            let other = camera.origin[1 - axis];
            let from = (((other / cell).floor() - 1.0) * cell - other) * zoom;
            let to = extent[1 - axis];
            for k in first..=last {
                let is_bold = step > 1 && k.rem_euclid(step) == 0;
                if !is_bold && cell * zoom < MIN_CELL {
                    continue;
                }
                let at = crisp((k as f64 * cell - origin) * zoom);
                if is_bold {
                    bar(from, at, to - from, thick, bold);
                    continue;
                }
                let mut along = from;
                while along < to {
                    bar(along, at, dash, thin, regular);
                    along += dash + gap;
                }
            }
        }
    }
}
