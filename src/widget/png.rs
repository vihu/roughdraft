//! A scene as a PNG, drawn without a window (`roughdraft render`).
use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::renderer::{self, Headless};
use iced::widget::canvas::Program;
use iced::{Color, Rectangle, Size, mouse};

use super::picture::encode_png;
use super::program::State;
use super::{Appearance, Sketch};
use crate::edit::common_bounds;
use crate::render::title_box;
use crate::scene::Scene;

/// Space around the content, as in Excalidraw's export
/// (`DEFAULT_EXPORT_PADDING`).
const PADDING: f64 = 10.0;

/// The longest side wgpu draws by default (`max_texture_dimension_2d`).
const MAX_SIDE: f32 = 8192.0;

/// A scene rendered to PNG.
#[derive(Debug)]
pub struct Png {
    /// The encoded file.
    pub bytes: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixels per scene unit: the scale asked for, or less where the image
    /// would be longer than 8192 pixels on a side.
    pub scale: f32,
    /// The renderer that drew it: `wgpu` or `tiny-skia`.
    pub renderer: String,
}

/// Why a scene did not render.
#[derive(Debug)]
#[non_exhaustive]
pub enum PngError {
    /// The scene has nothing to draw.
    Empty,
    /// Neither the GPU nor the CPU renderer started.
    NoRenderer,
    /// The pixels did not encode.
    Encode,
}

impl std::error::Error for PngError {}

impl std::fmt::Display for PngError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            PngError::Empty => "the scene has nothing to draw",
            PngError::NoRenderer => "no renderer started (neither wgpu nor tiny-skia)",
            PngError::Encode => "the image did not encode as PNG",
        })
    }
}

/// Draws `scene` as Excalidraw's PNG export frames it: everything,
/// frame titles included, plus 10 units around, at `scale` pixels per
/// unit, without the grid.
///
/// Draws on the GPU (wgpu) when there is one and on the CPU (tiny-skia)
/// otherwise; `ICED_BACKEND=tiny-skia` picks the CPU, as for the editor.
///
/// # Errors
///
/// Returns an error when the scene has nothing to draw, no renderer
/// starts, or the pixels do not encode.
pub fn render_png(scene: Scene, appearance: Appearance, scale: f32) -> Result<Png, PngError> {
    let live = || scene.elements.iter().filter(|e| !e.base.is_deleted);
    let titles = live().filter_map(|e| {
        let [x1, y1, x2, y2] = title_box(e)?;
        let [x, y] = [e.base.x, e.base.y];
        Some([x + x1, y + y1, x + x2, y + y2])
    });
    let [x1, y1, x2, y2] = titles.fold(common_bounds(live()), |[a, b, c, d], t| {
        [a.min(t[0]), b.min(t[1]), c.max(t[2]), d.max(t[3])]
    });
    if !x1.is_finite() {
        return Err(PngError::Empty);
    }
    let size = Size::new(
        (x2 - x1 + 2.0 * PADDING) as f32,
        (y2 - y1 + 2.0 * PADDING) as f32,
    );
    let scale = scale.min(MAX_SIDE / size.width.max(size.height));
    let mut sketch = Sketch::new(scene);
    sketch.grid = None;
    sketch.set_appearance(appearance);
    sketch.set_origin([x1 - PADDING, y1 - PADDING]);

    let backend = std::env::var("ICED_BACKEND").ok();
    let mut renderer = iced::futures::executor::block_on(iced::Renderer::new(
        renderer::Settings::default(),
        backend.as_deref(),
    ))
    .ok_or(PngError::NoRenderer)?;
    let layers = Program::draw(
        &sketch,
        &State::default(),
        &renderer,
        &iced::Theme::Light,
        Rectangle::with_size(size),
        mouse::Cursor::Unavailable,
    );
    for layer in layers {
        renderer.draw_geometry(layer);
    }
    let width = (size.width * scale).ceil() as u32;
    let height = (size.height * scale).ceil() as u32;
    let rgba = renderer.screenshot(Size::new(width, height), scale, Color::TRANSPARENT);
    Ok(Png {
        bytes: encode_png(width, height, rgba).ok_or(PngError::Encode)?,
        width,
        height,
        scale,
        renderer: renderer.name(),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Appearance, PngError, render_png};
    use crate::scene::Scene;

    fn scene(elements: serde_json::Value) -> Scene {
        serde_json::from_value(json!({"type": "excalidraw", "elements": elements})).unwrap()
    }

    fn rect(width: f64, height: f64) -> serde_json::Value {
        json!({"id": "r", "type": "rectangle", "x": 0, "y": 0, "width": width, "height": height})
    }

    #[test]
    fn the_image_is_the_content_plus_padding_at_the_scale() {
        let png = render_png(scene(json!([rect(100.0, 50.0)])), Appearance::Light, 2.0).unwrap();
        assert_eq!((png.width, png.height, png.scale), (240, 140, 2.0));
        assert!(png.bytes.starts_with(b"\x89PNG"));
    }

    #[test]
    fn a_long_scene_is_scaled_down_to_fit_and_an_empty_one_fails() {
        let png = render_png(scene(json!([rect(9980.0, 10.0)])), Appearance::Light, 2.0).unwrap();
        assert_eq!((png.width, png.scale), (8192, 8192.0 / 10000.0));
        assert!(matches!(
            render_png(scene(json!([])), Appearance::Light, 2.0),
            Err(PngError::Empty)
        ));
    }
}
