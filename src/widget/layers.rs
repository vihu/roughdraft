//! The scene's elements on a canvas frame: culled to the view, faded while
//! marked for erasing, and frame children clipped to their frame.
use iced::widget::canvas::Frame;

use super::picture::Picture;
use super::{Rendered, Sketch, paint};
use crate::geometry::{Affine, Bounds};
use crate::render::{self, Drawing, Item};

// Private API
impl Sketch {
    pub(super) fn draw_ids(&self, frame: &mut Frame, ids: &[String], view: Affine) {
        // Only what is in view, like Excalidraw's `getVisibleCanvasElements`.
        let camera = self.camera.get();
        let [ox, oy] = camera.origin;
        let size = frame.size();
        let (x2, y2) = (
            ox + f64::from(size.width) / camera.zoom,
            oy + f64::from(size.height) / camera.zoom,
        );
        let visible = |[a, b, c, d]: Bounds| a <= x2 && b <= y2 && c >= ox && d >= oy;
        // The element being typed is shown by the text overlay instead.
        let shown = ids.iter().filter(|id| self.editing.as_ref() != Some(*id));
        let erasing = self.editor.pending_erasure();
        let frames = render::frames(self.editor.scene());
        let drawings = shown
            .map(|id| (id, &self.drawings[id]))
            .filter(|(_, rendered)| visible(rendered.extent))
            .filter_map(|(id, rendered)| Some((id, rendered, rendered.drawing.as_ref()?)));
        // Frame children are cut off at their frame's box (`frameClip`).
        // Everything goes through clip drafts in runs of one region, since
        // wgpu draws a frame's own meshes after every pasted draft.
        // ponytail: square corners and unrotated, Excalidraw rounds them; and
        // iced's tiny-skia `paste` drops a draft's clip at the pinned rev, so
        // the software fallback draws frame children uncut
        let full = iced::Rectangle::with_size(frame.size());
        let region = |rendered: &Rendered| {
            let Some(owner) = rendered.frame.as_deref().and_then(|f| frames.get(f)) else {
                return full;
            };
            let b = &owner.base;
            let [x1, y1] = view.apply([b.x, b.y]);
            let [x2, y2] = view.apply([b.x + b.width, b.y + b.height]);
            iced::Rectangle {
                x: x1.min(x2) as f32,
                y: y1.min(y2) as f32,
                width: (x2 - x1).abs() as f32,
                height: (y2 - y1).abs() as f32,
            }
        };
        let mut runs: Vec<(iced::Rectangle, Vec<_>)> = Vec::new();
        for (id, rendered, drawing) in drawings {
            let clip = region(rendered);
            match runs.last_mut() {
                Some((last, run)) if *last == clip => run.push((id, rendered, drawing)),
                _ => runs.push((clip, vec![(id, rendered, drawing)])),
            }
        }
        for (clip, run) in runs {
            frame.with_clip(clip, |frame| {
                for (id, rendered, drawing) in run {
                    // `ELEMENT_READY_TO_ERASE_OPACITY`: 20%, children of a
                    // marked frame included (`getRenderOpacity`).
                    let marked = |marked: &std::collections::HashSet<String>| {
                        marked.contains(id)
                            || rendered.frame.as_ref().is_some_and(|f| marked.contains(f))
                    };
                    let fade = if erasing.is_some_and(marked) {
                        0.2
                    } else {
                        1.0
                    };
                    for drawing in rendered.label.iter().chain([drawing]) {
                        self.draw_drawing(frame, drawing, view, fade);
                    }
                }
            });
        }
    }

    fn draw_drawing(&self, frame: &mut Frame, drawing: &Drawing, view: Affine, fade: f32) {
        let transform = drawing.transform.then(view);
        for item in &drawing.items {
            match item {
                Item::Image {
                    file_id,
                    size,
                    opacity,
                    crop,
                    flip,
                } => {
                    let picture = Picture::of(file_id, crop.as_ref(), *flip);
                    self.draw_image(frame, &picture, *size, *opacity * fade, transform);
                }
                _ => paint::draw_item(frame, item, transform, &|c| self.paint(c.fade(fade))),
            }
        }
    }
}
