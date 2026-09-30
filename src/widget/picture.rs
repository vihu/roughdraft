//! Pictures: decoding files for the canvas, inserting image files, and
//! drawing image elements.
use iced::Color;
use iced::widget::canvas::{self, Frame};

use super::Sketch;
use crate::geometry::Affine;

// Public API
impl Sketch {
    /// Supplies the picture for an image file the scene does not embed (for
    /// hosts that store images elsewhere, such as Keeprs). `bytes` is an
    /// encoded image (PNG, JPEG, GIF, WebP); undecodable bytes keep the
    /// placeholder.
    pub fn set_image(&mut self, file_id: &str, bytes: &[u8]) {
        if let Some(handle) = decode_image(bytes) {
            self.images.insert(file_id.to_owned(), handle);
            self.clear_caches();
        }
    }

    /// Inserts an encoded image (PNG, JPEG, GIF, WebP) at the middle of the
    /// view, embedded in the scene as a data URL, like Excalidraw's image
    /// tool.
    ///
    /// # Errors
    ///
    /// Returns a message when the bytes are not an image this crate reads.
    pub fn insert_image(&mut self, bytes: &[u8]) -> Result<(), String> {
        let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let format = reader.format().ok_or("not a recognised image format")?;
        let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
        let mime = format.to_mime_type();
        let url = format!("data:{mime};base64,{}", crate::base64::encode(bytes));
        let viewport = self.viewport.get();
        let at = self.camera.scene_point(
            iced::Rectangle::with_size(viewport),
            iced::Point::new(viewport.width / 2.0, viewport.height / 2.0),
        );
        self.editor
            .insert_image(url, mime, [f64::from(width), f64::from(height)], at);
        self.refresh();
        Ok(())
    }
}

// Private API
impl Sketch {
    /// Draws a decoded image, or a grey placeholder while it is missing.
    /// Images keep their colors in dark mode, like Excalidraw's.
    pub(super) fn draw_image(
        &self,
        frame: &mut Frame,
        file_id: &str,
        size: [f64; 2],
        opacity: f32,
        transform: Affine,
    ) {
        let Some(handle) = self.images.get(file_id) else {
            let [w, h] = size;
            let corners = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]].map(|p| {
                let [x, y] = transform.apply(p);
                iced::Point::new(x as f32, y as f32)
            });
            let outline = canvas::Path::new(|path| {
                path.move_to(corners[0]);
                corners[1..].iter().for_each(|c| path.line_to(*c));
                path.close();
            });
            let grey = Color {
                a: 0.15 * opacity,
                ..Color::BLACK
            };
            frame.fill(&outline, grey);
            return;
        };
        // iced rotates the image about its centre, so place the unrotated box
        // around the transformed centre.
        let scale = transform.scale_factor();
        let [cx, cy] = transform.apply([size[0] / 2.0, size[1] / 2.0]);
        let (w, h) = (size[0] * scale, size[1] * scale);
        let bounds = iced::Rectangle {
            x: (cx - w / 2.0) as f32,
            y: (cy - h / 2.0) as f32,
            width: w as f32,
            height: h as f32,
        };
        let image = canvas::Image::new(handle.clone())
            .rotation(iced::Radians(transform.rotation() as f32))
            .opacity(opacity);
        frame.draw_image(bounds, image);
    }
}

/// Decodes an image to RGBA up front: iced draws RGBA handles in the frame
/// they appear, while encoded ones load on a worker and pop in later.
// ponytail: decodes on the UI thread when a scene loads; move to a Task if
// large photos stall opening a scene.
pub(super) fn decode_image(bytes: &[u8]) -> Option<iced::widget::image::Handle> {
    let rgba = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (width, height) = rgba.dimensions();
    Some(iced::widget::image::Handle::from_rgba(
        width,
        height,
        rgba.into_raw(),
    ))
}

/// Encodes RGBA pixels as PNG.
pub(super) fn encode_png(width: u32, height: u32, rgba: Vec<u8>) -> Option<Vec<u8>> {
    let image = image::RgbaImage::from_raw(width, height, rgba)?;
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(png)
}
