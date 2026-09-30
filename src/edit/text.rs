//! Text elements and labels inside shapes (REFERENCE-001 section 13;
//! `element/newElement.ts`, `element/textMeasurements.ts`, `App.startTextEditing`).
use std::f64::consts::SQRT_2;

use super::transform::{Frame, Handle};
use super::{Editor, Tool};
use crate::geometry::Point;
use crate::hit::{self, container_id};
use crate::scene::{BoundRef, Element, Kind, Text, TextAlign, VerticalAlign};

/// Measures text the way the renderer draws it (Excalidraw uses the
/// browser's `measureText`).
pub trait Measure: std::fmt::Debug {
    /// Returns the advance width of one line in an Excalidraw font.
    fn line_width(&self, line: &str, font_family: u32, font_size: f64) -> f64;
}

/// Fallback measure: every character is 0.55 em wide. Deterministic, for
/// tests and hosts without a text engine.
#[derive(Clone, Copy, Debug, Default)]
pub struct ApproxMeasure;

impl Measure for ApproxMeasure {
    fn line_width(&self, line: &str, _font_family: u32, font_size: f64) -> f64 {
        /// Average Excalifont advance: 11/20 = 0.55 em (as a fraction, so
        /// whole sizes give whole widths).
        const ADVANCE: (f64, f64) = (11.0, 20.0);
        line.chars().count() as f64 * font_size * ADVANCE.0 / ADVANCE.1
    }
}

/// A text element being typed into.
#[derive(Debug)]
pub(super) struct TextEdit {
    pub(super) index: usize,
    pub(super) before: Vec<Element>,
}

/// Gap between a container's edge and its label (`BOUND_TEXT_PADDING`).
const PADDING: f64 = 5.0;

/// Narrowest free text a side resize leaves (`getMinTextElementWidth`).
const MIN_TEXT_WIDTH: f64 = 2.0 * PADDING;

/// Clicking within this distance of a shape's centre with the text tool
/// labels the shape (`TEXT_TO_CENTER_SNAP_THRESHOLD`).
const CENTER_SNAP: f64 = 30.0;

pub(super) use crate::scene::line_height;

// Public API
impl Editor {
    /// Returns the text element being edited, if any.
    pub fn editing(&self) -> Option<&Element> {
        self.text.as_ref().map(|t| &self.scene.elements[t.index])
    }

    /// Uses `measure` for text sizes from now on.
    pub fn set_measure(&mut self, measure: Box<dyn Measure>) {
        self.measure = measure;
    }

    /// Double-click at `at`: edits the text or the shape's label there, or
    /// starts new text.
    pub fn double_click(&mut self, at: Point) {
        self.finish_text();
        self.finish_multi();
        self.line_edit = None;
        let target = self.hit(at).map(|e| e.base.id.clone());
        match target {
            // A grouped element: the first double-click enters its group.
            Some(id) if self.enter_group(&id) => {}
            Some(id) => {
                self.selected = std::iter::once(id.clone()).collect();
                self.edit_element(&id, at);
            }
            None => self.start_text(at, None),
        }
    }

    /// Replaces the edited text: free text is re-measured, labels are
    /// re-wrapped to their container, which grows to fit.
    pub fn set_text(&mut self, text: &str) {
        let Some(edit) = &self.text else { return };
        let index = edit.index;
        let original = text.replace("\r\n", "\n").replace('\t', "        ");
        self.layout_text(index, &original);
    }

    /// Ends editing. Empty text is deleted (and unbound from its container);
    /// otherwise the edit becomes one undo step and the text, or its
    /// container, is selected.
    pub fn finish_text(&mut self) {
        let Some(TextEdit { index, before }) = self.text.take() else {
            return;
        };
        let element = &self.scene.elements[index];
        let empty = matches!(&element.kind, Kind::Text(t) if t.text.trim().is_empty());
        let id = element.base.id.clone();
        let container = container_id(element).map(str::to_owned);
        let new = !before.iter().any(|e| e.base.id == id);
        if empty && new {
            // Nothing was typed into new text: the session leaves no trace
            // and no undo step.
            self.scene.elements.clone_from(&before);
        } else if empty {
            let element = &mut self.scene.elements[index];
            element.base.is_deleted = true;
            element.touch();
            let doomed = std::iter::once(id.clone()).collect();
            for other in self.scene.elements.iter_mut() {
                if other.forget_bindings(&doomed) {
                    other.touch();
                }
            }
        }
        if before != self.scene.elements {
            self.history.record(before);
        }
        self.selected.clear();
        // The container is selected, if it still exists (a cut or erase can
        // have taken it while its label was being typed).
        let shown = container.unwrap_or(id);
        let live = self
            .scene
            .elements
            .iter()
            .any(|e| e.base.id == shown && !e.base.is_deleted);
        if live {
            self.selected.insert(shown);
        }
    }
}

// Private API
impl Editor {
    /// Sets a text element's `originalText` and lays it out: free text is
    /// measured (keeping its alignment anchor), labels are wrapped to their
    /// container, which grows to fit.
    pub(super) fn layout_text(&mut self, index: usize, original: &str) {
        let original = original.to_owned();
        let container = container_id(&self.scene.elements[index])
            .and_then(|id| self.scene.elements.iter().position(|e| e.base.id == id));
        let Kind::Text(label) = &self.scene.elements[index].kind else {
            return;
        };
        let (family, size, align) = (label.font_family, label.font_size, label.text_align.clone());
        // Free text with a fixed width (after a side resize) wraps to it.
        let fixed = (container.is_none() && !self.scene.elements[index].auto_resize())
            .then_some(self.scene.elements[index].base.width);
        let shown = match (container, fixed) {
            (Some(c), _) => wrap(
                &original,
                max_label_width(&self.scene.elements[c], size),
                family,
                size,
                &*self.measure,
            ),
            (None, Some(width)) => wrap(&original, width, family, size, &*self.measure),
            (None, None) => original.clone(),
        };
        let (measured, height) = self.measure_block(&shown, family, size);
        let width = fixed.unwrap_or(measured);
        let element = &mut self.scene.elements[index];
        if let Kind::Text(text) = &mut element.kind {
            text.text = shown;
        }
        element
            .json_mut()
            .insert("originalText".into(), original.into());
        if container.is_none() {
            // Keep the alignment anchor: left edge, centre or right edge.
            let dx = element.base.width - width;
            element.base.x += match align {
                TextAlign::Center => dx / 2.0,
                TextAlign::Right => dx,
                _ => 0.0,
            };
        }
        element.base.width = width;
        element.base.height = height;
        element.touch();
        if let Some(c) = container {
            self.fit_container(c, height);
            let start = vec![(index, self.scene.elements[index].clone())];
            self.sync_labels(&start);
        }
    }

    /// Re-wraps a label to its container after a resize; a container that
    /// has to grow keeps its bottom edge when `from_top`.
    pub(super) fn rewrap_label(&mut self, index: usize, from_top: bool) {
        let Some(container) = container_id(&self.scene.elements[index])
            .and_then(|id| self.scene.elements.iter().position(|e| e.base.id == id))
        else {
            return;
        };
        let height = self.scene.elements[container].base.height;
        let original = self.scene.elements[index].original_text().to_owned();
        self.layout_text(index, &original);
        let grown = self.scene.elements[container].base.height - height;
        if from_top && grown > 0.0 {
            self.scene.elements[container].base.y -= grown;
            let start = vec![(index, self.scene.elements[index].clone())];
            self.sync_labels(&start);
        }
    }

    /// Sets free text's width from a side handle (`resizeSingleTextElement`,
    /// e and w): the text re-wraps at the same font size, the box keeps its
    /// top edge and the side across from the handle, and `autoResize` turns
    /// off so later edits wrap too. `pointer` is in the frame's coordinates.
    pub(super) fn resize_text_width(
        &mut self,
        index: usize,
        original: &Element,
        frame: &Frame,
        handle: Handle,
        pointer: Point,
    ) {
        let Kind::Text(text) = &original.kind else {
            return;
        };
        let [x1, y1, x2, _] = frame.bounds;
        let width = match handle {
            Handle::W => x2 - pointer[0],
            _ => pointer[0] - x1,
        }
        .max(MIN_TEXT_WIDTH);
        let (family, size) = (text.font_family, text.font_size);
        let shown = wrap(
            original.original_text(),
            width,
            family,
            size,
            &*self.measure,
        );
        let (_, height) = self.measure_block(&shown, family, size);
        let left = if handle == Handle::W { x2 - width } else { x1 };
        let [cx, cy] = frame
            .transform
            .apply([left + width / 2.0, y1 + height / 2.0]);
        let mut element = original.clone();
        if let Kind::Text(text) = &mut element.kind {
            text.text = shown;
        }
        element.base.x = cx - width / 2.0;
        element.base.y = cy - height / 2.0;
        element.base.width = width;
        element.base.height = height;
        element.json_mut().insert("autoResize".into(), false.into());
        element.touch();
        self.scene.elements[index] = element;
    }

    /// Pastes plain text as a new text element centred on `at`.
    pub(super) fn paste_text(&mut self, text: &str, at: Point) -> bool {
        if text.trim().is_empty() {
            return false;
        }
        let before = self.scene.elements.clone();
        self.start_text(at, None);
        self.set_text(text);
        let Some(edit) = self.text.take() else {
            return false;
        };
        let element = &mut self.scene.elements[edit.index];
        element.base.x -= element.base.width / 2.0;
        element.base.y -= element.base.height / 2.0;
        element.touch();
        self.history.record(before);
        self.selected = std::iter::once(element.base.id.clone()).collect();
        true
    }

    /// Text tool press: label the shape under `at` (or whose centre is
    /// within 30 units), else start free text.
    pub(super) fn text_press(&mut self, at: Point) {
        self.finish_text();
        // Free text under the click is edited (`getTextElementAtPosition`),
        // else a shape takes a label.
        let container = self
            .hit(at)
            .filter(|e| is_container(e) || matches!(e.kind, Kind::Text(_)))
            .or_else(|| {
                self.scene
                    .elements
                    .iter()
                    .rev()
                    .find(|e| is_container(e) && near_center(e, at))
            })
            .map(|e| e.base.id.clone());
        match container {
            Some(id) => self.edit_element(&id, at),
            None => self.start_text(at, None),
        }
        if !self.locked {
            self.tool = Tool::Selection;
        }
    }

    /// Edits a text element, or a container's label (created if missing).
    pub(super) fn edit_element(&mut self, id: &str, at: Point) {
        let Some(index) = self.scene.elements.iter().position(|e| e.base.id == id) else {
            return;
        };
        let element = &self.scene.elements[index];
        if matches!(element.kind, Kind::Text(_)) {
            self.text = Some(TextEdit {
                index,
                before: self.scene.elements.clone(),
            });
            return;
        }
        if !is_container(element) {
            return self.start_text(at, None);
        }
        let label = element
            .base
            .bound_elements
            .iter()
            .flatten()
            .find(|b| b.kind == "text")
            .map(|b| b.id.clone());
        let label_index = label.and_then(|l| {
            self.scene
                .elements
                .iter()
                .position(|e| e.base.id == l && !e.base.is_deleted)
        });
        match label_index {
            Some(label_index) => {
                self.text = Some(TextEdit {
                    index: label_index,
                    before: self.scene.elements.clone(),
                })
            }
            None => self.start_text(at, Some(index)),
        }
    }

    /// Creates an empty text element, free at `at` or as the label of the
    /// container at `container`, and starts editing it.
    pub(super) fn start_text(&mut self, at: Point, container: Option<usize>) {
        let before = self.scene.elements.clone();
        let style = self.style.clone();
        let family = style.font_family;
        let size = style.font_size;
        let line_height = line_height(family);
        let mut base = style.base(at);
        base.background_color = "transparent".into();
        base.height = size * line_height;
        let (align, vertical, container_id) = match container {
            Some(c) => (
                TextAlign::Center,
                VerticalAlign::Middle,
                Some(self.scene.elements[c].base.id.clone()),
            ),
            None => (style.text_align.clone(), VerticalAlign::Top, None),
        };
        if container.is_some() {
            base.stroke_color = self.style.stroke_color.clone();
        }
        let text = Text {
            text: String::new(),
            font_size: size,
            font_family: family,
            text_align: align,
            vertical_align: vertical,
            line_height,
            container_id,
        };
        let element = Element::new(Kind::Text(text), base);
        let id = element.base.id.clone();
        self.scene.elements.push(element);
        let index = self.scene.elements.len() - 1;
        if let Some(c) = container {
            let owner = &mut self.scene.elements[c];
            owner
                .base
                .bound_elements
                .get_or_insert_with(Vec::new)
                .push(BoundRef {
                    id,
                    kind: "text".into(),
                });
            owner.touch();
            self.fit_container(c, size * line_height);
            let start = vec![(index, self.scene.elements[index].clone())];
            self.sync_labels(&start);
        }
        self.text = Some(TextEdit { index, before });
    }

    /// Width of the widest line (empty lines measure as a space) and height
    /// of all lines, like `measureText`.
    fn measure_block(&self, text: &str, family: u32, size: f64) -> (f64, f64) {
        let lines: Vec<&str> = text.split('\n').collect();
        let width = lines
            .iter()
            .map(|line| {
                self.measure
                    .line_width(if line.is_empty() { " " } else { line }, family, size)
            })
            .fold(0.0, f64::max);
        (width, lines.len() as f64 * size * line_height(family))
    }

    /// Grows a container so a label of `label_height` fits
    /// (`getApproxMinLineHeight` and the container max-height rules).
    fn fit_container(&mut self, index: usize, label_height: f64) {
        let container = &self.scene.elements[index];
        let needed = match container.kind {
            Kind::Ellipse => (label_height + 2.0 * PADDING) * SQRT_2,
            Kind::Diamond => (label_height + 2.0 * PADDING) * 2.0,
            Kind::Arrow(_) => return,
            _ => label_height + 2.0 * PADDING,
        };
        if container.base.height < needed {
            let container = &mut self.scene.elements[index];
            container.base.height = needed;
            container.touch();
        }
    }
}

/// Shapes that can hold a label.
fn is_container(element: &Element) -> bool {
    !element.base.is_deleted
        && hit::container_id(element).is_none()
        && matches!(
            element.kind,
            Kind::Rectangle | Kind::Diamond | Kind::Ellipse | Kind::Arrow(_)
        )
}

fn near_center(element: &Element, at: Point) -> bool {
    let b = &element.base;
    let center = [b.x + b.width / 2.0, b.y + b.height / 2.0];
    (center[0] - at[0]).hypot(center[1] - at[1]) <= CENTER_SNAP
        && !matches!(element.kind, Kind::Arrow(_))
}

/// Widest a label may be inside its container (`getBoundTextMaxWidth`).
pub(super) fn max_label_width(container: &Element, font_size: f64) -> f64 {
    /// Arrow labels are at least this many ems wide.
    const ARROW_MIN_EMS: f64 = 11.0;
    /// Arrow labels are at most this share of the arrow's width.
    const ARROW_SHARE: f64 = 0.7;

    let w = container.base.width;
    match container.kind {
        Kind::Ellipse => (w / 2.0 * SQRT_2).round() - 2.0 * PADDING,
        Kind::Diamond => (w / 2.0).round() - 2.0 * PADDING,
        Kind::Arrow(_) => (w * ARROW_SHARE).max(font_size * ARROW_MIN_EMS),
        _ => w - 2.0 * PADDING,
    }
}

/// Greedy word wrap to `max_width`; words wider than a line are broken
/// between characters.
fn wrap(text: &str, max_width: f64, family: u32, size: f64, measure: &dyn Measure) -> String {
    let fits = |s: &str| measure.line_width(s, family, size) <= max_width;
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if fits(&candidate) || line.is_empty() && word.is_empty() {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            for c in word.chars() {
                let candidate = format!("{line}{c}");
                if !line.is_empty() && !fits(&candidate) {
                    lines.push(std::mem::take(&mut line));
                    line.push(c);
                } else {
                    line = candidate;
                }
            }
        }
        lines.push(line);
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{ApproxMeasure, wrap};

    #[test]
    fn wrap_breaks_on_spaces_then_inside_long_words() {
        // 0.55 * 20 = 11 units per char; 60 units fit 5 chars.
        let wrapped = wrap("ab cd efghijkl", 60.0, 5, 20.0, &ApproxMeasure);
        assert_eq!(wrapped, "ab cd\nefghi\njkl");
        assert_eq!(
            wrap("one\n\ntwo", 1000.0, 5, 20.0, &ApproxMeasure),
            "one\n\ntwo"
        );
    }
}
