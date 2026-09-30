//! Pan and zoom, with Excalidraw's keyboard zoom (`actions/actionCanvas.tsx`).
use iced::keyboard::{self, key::Code};

use crate::geometry::{self, Affine, Bounds};

/// The scene point at the canvas' top-left, and the scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Camera {
    pub(super) origin: [f64; 2],
    pub(super) zoom: f64,
}

/// A keyboard zoom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ZoomKey {
    /// Ctrl or Shift with `=`: 10 points in.
    In,
    /// Ctrl or Shift with `-`: 10 points out.
    Out,
    /// Ctrl or Shift with `0`: 100%.
    Reset,
    /// Shift+1: everything, at most 100%.
    FitAll,
    /// Shift+2: the selection (everything when empty), at most 100%.
    FitSelection,
    /// Shift+3: the selection (everything when empty), filling the view.
    FillSelection,
}

/// Excalidraw's zoom limits.
const ZOOM: std::ops::RangeInclusive<f64> = 0.1..=30.0;

/// `ZOOM_STEP`.
const ZOOM_STEP: f64 = 0.1;

impl Camera {
    /// Scene to canvas pixels.
    pub(super) fn view(&self) -> Affine {
        let [x, y] = self.origin;
        Affine::translate([-x, -y]).then(Affine::scale(self.zoom))
    }

    /// Scene point under a window position.
    pub(super) fn scene_point(
        &self,
        bounds: iced::Rectangle,
        position: iced::Point,
    ) -> geometry::Point {
        let [x, y] = self.origin;
        [
            x + f64::from(position.x - bounds.x) / self.zoom,
            y + f64::from(position.y - bounds.y) / self.zoom,
        ]
    }

    /// Zooms to `zoom` (`getNormalizedZoom`: 6 decimals, within limits),
    /// keeping the scene point under `cursor` (canvas pixels) in place.
    pub(super) fn zoom_about(&mut self, zoom: f64, cursor: [f64; 2]) {
        let zoom = ((zoom * 1e6).round() / 1e6).clamp(*ZOOM.start(), *ZOOM.end());
        self.origin = zoom_at(self.origin, self.zoom, zoom, cursor);
        self.zoom = zoom;
    }

    /// Applies a keyboard zoom in a `viewport` (canvas pixels). `targets` is
    /// the box of what the fit keys frame, `None` when the scene is empty.
    pub(super) fn apply(&mut self, key: ZoomKey, viewport: [f64; 2], targets: Option<Bounds>) {
        let centre = [viewport[0] / 2.0, viewport[1] / 2.0];
        let max = match key {
            ZoomKey::In => return self.zoom_about(self.zoom + ZOOM_STEP, centre),
            ZoomKey::Out => return self.zoom_about(self.zoom - ZOOM_STEP, centre),
            ZoomKey::Reset => return self.zoom_about(1.0, centre),
            ZoomKey::FitAll | ZoomKey::FitSelection => 1.0,
            ZoomKey::FillSelection => f64::INFINITY,
        };
        let Some([x1, y1, x2, y2]) = targets else {
            return;
        };
        // `zoomToFitBounds`: floored to a 10% step (`roundToStep`), centred
        // on the box.
        let fit = (viewport[0] / (x2 - x1))
            .min(viewport[1] / (y2 - y1))
            .min(max);
        let factor = 1.0 / ZOOM_STEP;
        self.zoom_about((fit * factor).floor() / factor, centre);
        let (cx, cy) = ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
        self.origin = [cx - centre[0] / self.zoom, cy - centre[1] / self.zoom];
    }
}

/// Returns the keyboard zoom for a physical key, which Excalidraw matches by
/// key code so Shift+1 works on every layout.
pub(super) fn zoom_key(code: Code, modifiers: keyboard::Modifiers) -> Option<ZoomKey> {
    let (command, shift, alt) = (modifiers.command(), modifiers.shift(), modifiers.alt());
    let fit = shift && !command && !alt;
    match code {
        Code::Equal | Code::NumpadAdd if command || shift => Some(ZoomKey::In),
        Code::Minus | Code::NumpadSubtract if command || shift => Some(ZoomKey::Out),
        Code::Digit0 | Code::Numpad0 if command || shift => Some(ZoomKey::Reset),
        Code::Digit1 if fit => Some(ZoomKey::FitAll),
        Code::Digit2 if fit => Some(ZoomKey::FitSelection),
        Code::Digit3 if fit => Some(ZoomKey::FillSelection),
        _ => None,
    }
}

/// Returns the origin that keeps the scene point under `cursor` (canvas
/// pixels) fixed while zooming from `from` to `to`.
fn zoom_at(origin: [f64; 2], from: f64, to: f64, cursor: [f64; 2]) -> [f64; 2] {
    let anchor = [origin[0] + cursor[0] / from, origin[1] + cursor[1] / from];
    [anchor[0] - cursor[0] / to, anchor[1] - cursor[1] / to]
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{Modifiers, key::Code};

    use super::{Camera, ZoomKey, zoom_at, zoom_key};

    #[test]
    fn zoom_keeps_point_under_cursor() {
        let (origin, cursor) = ([-10.0, 40.0], [300.0, 200.0]);
        let under = |origin: [f64; 2], zoom: f64| {
            [origin[0] + cursor[0] / zoom, origin[1] + cursor[1] / zoom]
        };
        let zoomed = zoom_at(origin, 1.0, 2.5, cursor);
        assert_eq!(under(zoomed, 2.5), under(origin, 1.0));
        assert_eq!(zoom_at(origin, 2.0, 2.0, cursor), origin);
    }

    #[test]
    fn keyboard_zoom_steps_fits_and_clamps_like_excalidraw() {
        let viewport = [800.0, 600.0];
        let mut camera = Camera {
            origin: [0.0, 0.0],
            zoom: 1.0,
        };
        camera.apply(ZoomKey::In, viewport, None);
        camera.apply(ZoomKey::In, viewport, None);
        assert_eq!(camera.zoom, 1.2, "no float drift");
        // The view centre (scene 400, 300) stays put.
        assert_eq!(
            camera.scene_point(iced::Rectangle::default(), iced::Point::new(400.0, 300.0)),
            [400.0, 300.0]
        );
        for _ in 0..20 {
            camera.apply(ZoomKey::Out, viewport, None);
        }
        assert_eq!(camera.zoom, 0.1);

        // 400x100 content: 2x would fit, "fit" stops at 100%, "fill" floors 2.0.
        let content = Some([100.0, 100.0, 500.0, 200.0]);
        camera.apply(ZoomKey::FitAll, viewport, content);
        assert_eq!((camera.zoom, camera.origin), (1.0, [-100.0, -150.0]));
        camera.apply(ZoomKey::FillSelection, viewport, content);
        assert_eq!((camera.zoom, camera.origin), (2.0, [100.0, 0.0]));
        // 0.75 floors to 0.7.
        camera.apply(
            ZoomKey::FitAll,
            viewport,
            Some([0.0, 0.0, 800.0 / 0.75, 10.0]),
        );
        assert_eq!(camera.zoom, 0.7);
        camera.apply(ZoomKey::FitAll, viewport, None);
        assert_eq!(camera.zoom, 0.7, "empty scene keeps the view");
    }

    #[test]
    fn zoom_keys_match_by_code() {
        let shift = Modifiers::SHIFT;
        assert_eq!(zoom_key(Code::Equal, Modifiers::CTRL), Some(ZoomKey::In));
        assert_eq!(zoom_key(Code::Minus, shift), Some(ZoomKey::Out));
        assert_eq!(
            zoom_key(Code::Digit0, Modifiers::CTRL),
            Some(ZoomKey::Reset)
        );
        assert_eq!(zoom_key(Code::Digit1, shift), Some(ZoomKey::FitAll));
        assert_eq!(zoom_key(Code::Digit3, shift), Some(ZoomKey::FillSelection));
        assert_eq!(
            zoom_key(Code::Digit1, Modifiers::empty()),
            None,
            "1 picks the selection tool"
        );
        assert_eq!(zoom_key(Code::Digit1, shift | Modifiers::ALT), None);
    }
}
