//! Pictures: decoding files for the canvas, inserting image files, and
//! drawing image elements.
use iced::Color;
use iced::widget::canvas::{self, Frame};

use super::Sketch;
use crate::geometry::Affine;
use crate::render::Crop;

/// A decoded picture: a file, the part of it shown, and its mirroring.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct Picture {
    file: String,
    /// Crop rectangle in whole file pixels, `None` for the whole file.
    crop: Option<[i64; 4]>,
    flip: [bool; 2],
}

// Public API
impl Sketch {
    /// Supplies the picture for an image file the scene does not embed (for
    /// hosts that store images elsewhere, such as Keeprs). `bytes` is an
    /// encoded image (PNG, JPEG, GIF, WebP); undecodable bytes keep the
    /// placeholder.
    pub fn set_image(&mut self, file_id: &str, bytes: &[u8]) {
        if let Some(handle) = decode_image(bytes) {
            // Cut or mirrored views of the old picture are made again.
            self.images.retain(|picture, _| picture.file != file_id);
            self.images
                .insert(Picture::of(file_id, None, [false, false]), handle);
            self.refresh();
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
        let at = self.camera.get().scene_point(
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
        picture: &Picture,
        size: [f64; 2],
        opacity: f32,
        transform: Affine,
    ) {
        let Some(handle) = self.images.get(picture) else {
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

impl Picture {
    /// The picture an image element shows.
    pub(super) fn of(file: &str, crop: Option<&Crop>, flip: [bool; 2]) -> Self {
        Self {
            file: file.to_owned(),
            crop: crop.map(|c| c.rect.map(|v| v.round() as i64)),
            flip,
        }
    }

    /// Cuts and mirrors the whole picture's pixels (`drawImage` with a
    /// source rectangle, then `scale(-1, 1)`); the crop is scaled from the
    /// natural size it was made for to the decoded one.
    pub(super) fn derive(
        &self,
        whole: &iced::widget::image::Handle,
        crop: Option<&Crop>,
    ) -> Option<iced::widget::image::Handle> {
        let iced::widget::image::Handle::Rgba {
            width,
            height,
            pixels,
            ..
        } = whole
        else {
            return None;
        };
        let (width, height) = (*width as usize, *height as usize);
        let [x, y, w, h] = match crop {
            Some(crop) => {
                let (sx, sy) = (
                    width as f64 / crop.natural[0],
                    height as f64 / crop.natural[1],
                );
                let [x, y, w, h] = crop.rect;
                let x0 = ((x * sx).round().max(0.0) as usize).min(width - 1);
                let y0 = ((y * sy).round().max(0.0) as usize).min(height - 1);
                let w0 = ((w * sx).round().max(1.0) as usize).min(width - x0);
                let h0 = ((h * sy).round().max(1.0) as usize).min(height - y0);
                [x0, y0, w0, h0]
            }
            None => [0, 0, width, height],
        };
        let mut out = Vec::with_capacity(w * h * 4);
        for row in 0..h {
            let from_row = y + if self.flip[1] { h - 1 - row } else { row };
            for col in 0..w {
                let from_col = x + if self.flip[0] { w - 1 - col } else { col };
                let i = (from_row * width + from_col) * 4;
                out.extend_from_slice(&pixels[i..i + 4]);
            }
        }
        Some(iced::widget::image::Handle::from_rgba(
            w as u32, h as u32, out,
        ))
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

#[cfg(test)]
mod tests {
    use super::Picture;
    use crate::render::Crop;

    #[test]
    fn crops_scale_to_the_decoded_size_and_flips_mirror() {
        // 4 x 2 pixels, red channel = index.
        let pixels: Vec<u8> = (0..8u8).flat_map(|i| [i, 0, 0, 255]).collect();
        let whole = iced::widget::image::Handle::from_rgba(4, 2, pixels);
        let red = |handle: iced::widget::image::Handle| match handle {
            iced::widget::image::Handle::Rgba {
                width,
                height,
                pixels,
                ..
            } => (
                (width, height),
                pixels.chunks(4).map(|p| p[0]).collect::<Vec<_>>(),
            ),
            _ => unreachable!(),
        };
        // The right half, given in a natural size twice the decoded one.
        let crop = Crop {
            rect: [4.0, 0.0, 4.0, 4.0],
            natural: [8.0, 4.0],
        };
        let cut = Picture::of("f", Some(&crop), [false, false])
            .derive(&whole, Some(&crop))
            .unwrap();
        assert_eq!(red(cut), ((2, 2), vec![2, 3, 6, 7]));
        let mirrored = Picture::of("f", None, [true, true])
            .derive(&whole, None)
            .unwrap();
        assert_eq!(red(mirrored), ((4, 2), vec![7, 6, 5, 4, 3, 2, 1, 0]));
    }
}
