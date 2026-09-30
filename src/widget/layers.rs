//! The scene's elements as drawings: re-rendered when they change, drawn on
//! a canvas frame culled to the view, faded while marked for erasing, and
//! frame children clipped to their frame.
use std::collections::{HashMap, HashSet};

use iced::widget::canvas::Frame;

use super::picture::{Picture, decode_image};
use super::{Rendered, Sketch, paint};
use crate::edit;
use crate::geometry::{self, Affine, Bounds};
use crate::render::{self, Drawing, Item, Segment};
use crate::scene::{Element, Scene};

/// Gap between the content and the canvas edge on open, like Excalidraw's
/// SVG export padding.
const PADDING: f64 = 10.0;

/// Scene units added around an element's box before culling it: covers the
/// rough wobble, stroke width and arrowheads, which reach past the box.
const CULL_MARGIN: f64 = 50.0;

// Private API
impl Sketch {
    /// Re-renders elements whose revision changed and invalidates the
    /// static layers when anything outside the current gesture changed.
    pub(super) fn refresh(&mut self) {
        let scene = self.editor.scene();
        let background = scene.background_color();
        let active: Vec<String> = self.editor.active().into_iter().map(String::from).collect();
        let mut previous = std::mem::take(&mut self.drawings);
        let mut order = Vec::new();
        let mut static_changed = active != self.active;

        for element in render::draw_order(scene) {
            let id = &element.base.id;
            let revision = revision(element, scene);
            let live = active.contains(id);
            let rendered = match previous.remove(id) {
                // A stale drawing is only good while its gesture lasts.
                Some(cached) if cached.revision == revision && (live || !cached.stale) => cached,
                _ => {
                    // A frame resized alone re-clips children that sit on
                    // the static layers.
                    let clips_static = element.frame_title().is_some()
                        && scene.elements.iter().any(|e| {
                            !e.base.is_deleted
                                && e.frame_id() == Some(id.as_str())
                                && !active.contains(&e.base.id)
                        });
                    static_changed |= !active.contains(id) || clips_static;
                    let [x1, y1, x2, y2] = geometry::element_bounds(element);
                    Rendered {
                        revision,
                        drawing: (!live)
                            .then(|| draw_element(element, scene, background))
                            .flatten(),
                        stale: live,
                        label: render::frame_label(element),
                        frame: element.frame_id().map(String::from),
                        extent: [
                            x1 - CULL_MARGIN,
                            y1 - CULL_MARGIN,
                            x2 + CULL_MARGIN,
                            y2 + CULL_MARGIN,
                        ],
                    }
                }
            };
            if rendered.drawing.is_some() || rendered.stale {
                order.push(id.clone());
            }
            self.drawings.insert(id.clone(), rendered);
        }
        for element in scene.elements.iter().filter(|e| !e.base.is_deleted) {
            let Some(id) = element.file_id() else {
                continue;
            };
            let whole = Picture::of(id, None, [false, false]);
            if !self.images.contains_key(&whole) && !self.undecodable.contains(id) {
                let handle = scene
                    .file_data_url(id)
                    .and_then(crate::base64::decode_data_url)
                    .and_then(|(_, bytes)| decode_image(&bytes));
                match handle {
                    Some(handle) => {
                        self.images.insert(whole.clone(), handle);
                        static_changed = true;
                    }
                    // A file the scene does not hold yet may come later
                    // (`set_image`, a paste that brings it).
                    None if scene.file_data_url(id).is_some() => {
                        self.undecodable.insert(id.to_owned());
                    }
                    None => {}
                }
            }
            // A cut or mirrored view comes from the whole picture's pixels.
            let crop = element
                .image_crop()
                .map(|(rect, natural)| render::Crop { rect, natural });
            let picture = Picture::of(id, crop.as_ref(), element.image_flip());
            if picture != whole
                && !self.images.contains_key(&picture)
                && let Some(handle) = self
                    .images
                    .get(&whole)
                    .and_then(|w| picture.derive(w, crop.as_ref()))
            {
                self.images.insert(picture, handle);
                static_changed = true;
            }
        }
        let grid = scene.grid();
        static_changed |= !previous.is_empty() || order != self.order || grid != self.grid;
        self.grid = grid;
        self.order = order;
        self.active = active;
        if static_changed {
            self.clear_caches();
        }
    }

    /// Renders the stale elements among `ids` for this frame.
    pub(super) fn render_stale(&self, ids: &[String]) -> HashMap<&str, Drawing> {
        let stale: HashSet<&str> = ids
            .iter()
            .map(String::as_str)
            .filter(|id| self.drawings[*id].stale)
            .collect();
        if stale.is_empty() {
            return HashMap::new();
        }
        let scene = self.editor.scene();
        let background = scene.background_color();
        scene
            .elements
            .iter()
            .filter(|e| stale.contains(e.base.id.as_str()))
            .filter_map(|e| {
                let drawing = draw_element(e, scene, background)?;
                Some((e.base.id.as_str(), drawing))
            })
            .collect()
    }

    pub(super) fn drawings(&self) -> impl Iterator<Item = &Drawing> {
        self.order
            .iter()
            .filter_map(|id| self.drawings[id].drawing.as_ref())
    }

    /// Centres the live elements in a view of `size` at the camera's zoom.
    pub(super) fn centre_content(&self, size: iced::Size) {
        let live: Vec<&crate::scene::Element> = self
            .editor
            .scene()
            .elements
            .iter()
            .filter(|e| !e.base.is_deleted)
            .collect();
        if live.is_empty() {
            return;
        }
        let [x1, y1, x2, y2] = edit::common_bounds(live.into_iter());
        let mut camera = self.camera.get();
        camera.origin = [
            (x1 + x2) / 2.0 - f64::from(size.width) / 2.0 / camera.zoom,
            (y1 + y2) / 2.0 - f64::from(size.height) / 2.0 / camera.zoom,
        ];
        self.camera.set(camera);
    }

    /// Draws the elements `ids` in view. A stale element (changed by the
    /// gesture) is drawn from `fresh`, rendered for this frame.
    pub(super) fn draw_ids(
        &self,
        frame: &mut Frame,
        ids: &[String],
        view: Affine,
        fresh: &HashMap<&str, Drawing>,
    ) {
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
            .filter_map(|(id, rendered)| {
                let drawing = fresh.get(id.as_str()).or(rendered.drawing.as_ref())?;
                Some((id, rendered, drawing))
            });
        // Frame children are cut off at their frame's box (`frameClip`).
        // Everything goes through clip drafts in runs of one region, since
        // wgpu draws a frame's own meshes after every pasted draft.
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

/// Top-left of everything drawn, minus [`PADDING`], so a scene opens at 100%
/// zoom aligned like Excalidraw's SVG export.
pub(super) fn content_origin<'a>(drawings: impl Iterator<Item = &'a Drawing>) -> [f64; 2] {
    let mut min = [f64::INFINITY, f64::INFINITY];
    for drawing in drawings {
        for item in &drawing.items {
            let points: Vec<[f64; 2]> = match item {
                Item::Stroke { path, .. } | Item::Fill { path, .. } => path
                    .iter()
                    .map(|segment| match *segment {
                        Segment::MoveTo(p) | Segment::LineTo(p) | Segment::CubicTo(_, _, p) => p,
                    })
                    .collect(),
                Item::Text(_) => vec![[0.0, 0.0]],
                Item::Image { size, .. } | Item::Frame { size, .. } => vec![[0.0, 0.0], *size],
            };
            for p in points {
                let [x, y] = drawing.transform.apply(p);
                min = [min[0].min(x), min[1].min(y)];
            }
        }
    }
    if min[0].is_finite() {
        [min[0] - PADDING, min[1] - PADDING]
    } else {
        [0.0, 0.0]
    }
}

/// Renders an element as the canvas shows it: a labelled arrow's line
/// stops short of its label.
fn draw_element(element: &Element, scene: &Scene, background: &str) -> Option<Drawing> {
    let drawing = render::render_element(element, background)?;
    Some(match render::arrow_label(element, scene) {
        Some(label) => render::cut_gap(&drawing, render::label_gap(label)),
        None => drawing,
    })
}

/// An element's revision for the drawing cache; an arrow's includes its
/// label's, since the label cuts its line.
fn revision(element: &Element, scene: &Scene) -> (i64, i64) {
    let (version, nonce) = element.revision();
    match render::arrow_label(element, scene) {
        Some(label) => {
            let (label_version, label_nonce) = label.revision();
            (version.wrapping_add(label_version), nonce ^ label_nonce)
        }
        None => (version, nonce),
    }
}
