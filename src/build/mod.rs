//! Complete Excalidraw scenes from short skeletons, for agents and scripts
//! (`roughdraft build`, PLAN-002).
//!
//! The input is Excalidraw's element skeleton (`convertToExcalidrawElements`,
//! 0.18 `data/transform.ts`) with ids kept, a label as a plain string, arrows
//! placed between the shapes they name, and `row` / `column` groups that lay
//! their children out. The output has every field Excalidraw writes, text
//! measured, labels fitted and both sides of every binding.
mod layout;
mod parse;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value, json};

use crate::edit::{Editor, Measure, Style, max_label_width};
use crate::scene::{
    ArrowEnd, Arrowhead, BoundRef, Element, Kind, Linear, Scene, Text, TextAlign, VerticalAlign,
    line_height,
};

use self::layout::Node;
use self::parse::{Item, Parse};

/// A built scene and what was changed or ignored on the way.
#[derive(Debug)]
pub struct Built {
    /// The complete scene.
    pub scene: Scene,
    /// Parts of the input that were ignored or adjusted.
    pub warnings: Vec<Issue>,
}

/// A problem with one part of the input.
#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    /// Where in the input, like `elements[2].label`.
    pub path: String,
    /// What is wrong, in words.
    pub message: String,
}

/// Builds a scene from a skeleton: a JSON array of items, or an object with
/// `elements` (and optionally `appState`). `measure` sizes the text.
///
/// # Errors
///
/// Returns every problem found when the input cannot be built: unknown
/// types, duplicate or missing ids, arrows that cannot be placed, and
/// malformed values.
pub fn build(input: &Value, measure: Box<dyn Measure>) -> Result<Built, Vec<Issue>> {
    let mut parse = Parse::default();
    let (items, app_state) = match input {
        Value::Array(items) => (items.as_slice(), None),
        Value::Object(object) => match object.get("elements") {
            Some(Value::Array(items)) => (items.as_slice(), object.get("appState")),
            _ => {
                return Err(vec![issue(
                    "",
                    "expected an array of items or {\"elements\": [...]}",
                )]);
            }
        },
        _ => {
            return Err(vec![issue(
                "",
                "expected an array of items or {\"elements\": [...]}",
            )]);
        }
    };
    let roots: Vec<Node> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| parse.item(item, &format!("elements[{i}]"), false))
        .collect();
    parse.check_references();
    if !parse.errors.is_empty() {
        return Err(parse.errors);
    }

    let mut scene: Scene = serde_json::from_value(json!({
        "type": "excalidraw",
        "version": 2,
        "source": "roughdraft",
        "elements": [],
        "appState": app_state.cloned().unwrap_or_else(|| json!({ "gridSize": 20, "viewBackgroundColor": "#ffffff" })),
        "files": {},
    }))
    .map_err(|e| vec![issue("appState", &e.to_string())])?;
    for item in &parse.items {
        match make_element(item) {
            Ok(elements) => scene.elements.extend(elements),
            Err(message) => parse.errors.push(issue(&item.path, &message)),
        }
    }
    if !parse.errors.is_empty() {
        return Err(parse.errors);
    }

    let mut editor = Editor::new(scene);
    editor.set_measure(measure);
    let mut builder = Builder {
        editor,
        warnings: parse.warnings,
    };
    builder.size_text_and_shapes(&parse.items);
    builder.place(&roots);
    builder.lay_out_labels(&parse.items, |item| item.kind != "arrow");
    builder.fit_frames(&parse.items);
    builder.route_arrows(&parse.items)?;
    builder.lay_out_labels(&parse.items, |item| item.kind == "arrow");
    let warnings = builder.warnings;
    Ok(Built {
        scene: builder.editor.scene().clone(),
        warnings,
    })
}

/// Default size of a shape without a label (`DEFAULT_DIMENSION`).
const DEFAULT_SIZE: f64 = 100.0;

/// Distance between an arrow end and the outline of its shape.
const ARROW_GAP: f64 = 6.0;

/// `BOUND_TEXT_PADDING`: the gap between a shape's edge and its label.
const PADDING: f64 = 5.0;

/// Keys of an item that are the skeleton's own, not element fields.
const SKELETON_KEYS: [&str; 4] = ["label", "start", "end", "children"];

/// The element for an item, and its label when it has one, with every
/// field Excalidraw writes for a new element and the item's own on top.
fn make_element(item: &Item) -> Result<Vec<Element>, String> {
    let number = |key: &str| item.json.get(key).and_then(Value::as_f64);
    let at = [number("x").unwrap_or(0.0), number("y").unwrap_or(0.0)];
    let base = Style::default().base(at);
    let kind = match item.kind.as_str() {
        "rectangle" => Kind::Rectangle,
        "ellipse" => Kind::Ellipse,
        "diamond" => Kind::Diamond,
        "text" => Kind::Text(text_kind(&item.json, None)),
        "arrow" => Kind::Arrow(Linear {
            points: vec![[0.0, 0.0], [DEFAULT_SIZE, 0.0]],
            start_arrowhead: None,
            end_arrowhead: Some(Arrowhead::Arrow),
        }),
        "line" => Kind::Line(Linear {
            points: vec![[0.0, 0.0], [DEFAULT_SIZE, 0.0]],
            start_arrowhead: None,
            end_arrowhead: None,
        }),
        _ => Kind::Other("frame".into()),
    };
    let mut map: Map<String, Value> = Element::new(kind, base).into();
    if item.kind == "frame" {
        map.insert("name".into(), Value::Null);
    }
    for (key, value) in &item.json {
        if !SKELETON_KEYS.contains(&key.as_str()) {
            map.insert(key.clone(), value.clone());
        }
    }
    map.insert("id".into(), item.id.clone().into());
    if item.kind == "text" {
        map.insert("originalText".into(), map["text"].clone());
    }
    let shape = matches!(item.kind.as_str(), "rectangle" | "ellipse" | "diamond");
    // Sized to fit its label later, like Excalidraw's skeleton.
    let fit = shape && item.label.is_some();
    for key in ["width", "height"] {
        if shape && !item.json.contains_key(key) {
            map.insert(key.into(), json!(if fit { 0.0 } else { DEFAULT_SIZE }));
        }
    }
    let mut element = Element::try_from(map).map_err(|e| e.to_string())?;
    if let Kind::Line(line) | Kind::Arrow(line) = &element.kind {
        let span = |axis: usize| {
            let values = line.points.iter().map(|p| p[axis]);
            values.clone().fold(f64::NEG_INFINITY, f64::max) - values.fold(f64::INFINITY, f64::min)
        };
        if line.points.len() < 2 {
            return Err("\"points\" needs at least two points".into());
        }
        (element.base.width, element.base.height) = (span(0), span(1));
    }
    let Some(label) = &item.label else {
        return Ok(vec![element]);
    };
    let label_id = format!("{}-label", item.id);
    let mut text_base = Style::default().base(at);
    text_base.id = label_id.clone();
    text_base.stroke_color = label
        .get("strokeColor")
        .and_then(Value::as_str)
        .unwrap_or(&element.base.stroke_color)
        .to_owned();
    let mut text: Map<String, Value> =
        Element::new(Kind::Text(text_kind(label, Some(&item.id))), text_base).into();
    for (key, value) in label {
        text.insert(key.clone(), value.clone());
    }
    text.insert("originalText".into(), label["text"].clone());
    text.insert("containerId".into(), item.id.clone().into());
    element
        .base
        .bound_elements
        .get_or_insert_with(Vec::new)
        .push(BoundRef {
            id: label_id,
            kind: "text".into(),
        });
    Ok(vec![
        element,
        Element::try_from(text).map_err(|e| e.to_string())?,
    ])
}

/// A text element's typed fields: free text reads left and top, a label
/// centred (Excalidraw's defaults for each).
fn text_kind(json: &Map<String, Value>, container: Option<&str>) -> Text {
    let family = json.get("fontFamily").and_then(Value::as_u64).unwrap_or(5) as u32;
    let (align, vertical) = match container {
        Some(_) => (TextAlign::Center, VerticalAlign::Middle),
        None => (TextAlign::Left, VerticalAlign::Top),
    };
    Text {
        text: json["text"].as_str().unwrap_or_default().to_owned(),
        font_size: json.get("fontSize").and_then(Value::as_f64).unwrap_or(20.0),
        font_family: family,
        text_align: align,
        vertical_align: vertical,
        line_height: line_height(family),
        container_id: container.map(String::from),
    }
}

/// The scene under construction, in an editor for its text layout and
/// arrow binding.
struct Builder {
    editor: Editor,
    warnings: Vec<Issue>,
}

impl Builder {
    fn index(&self, id: &str) -> usize {
        self.editor
            .scene()
            .elements
            .iter()
            .position(|e| e.base.id == id)
            .expect("every item made an element")
    }

    /// Measures free text, fits shapes without a size to their labels
    /// (`computeContainerDimensionForBoundText`), and wraps labels to their
    /// shapes, which grow taller to fit.
    fn size_text_and_shapes(&mut self, items: &[Item]) {
        for item in items {
            let index = self.index(&item.id);
            if item.kind == "text" {
                let original = self.editor.scene().elements[index]
                    .original_text()
                    .to_owned();
                self.editor.layout_text(index, &original);
                continue;
            }
            let Some(label) = &item.label else { continue };
            if item.kind == "arrow" {
                continue;
            }
            let text = label["text"].as_str().unwrap_or_default().to_owned();
            let label_index = index + 1;
            if !item.json.contains_key("width") {
                let Kind::Text(t) = &self.editor.scene().elements[label_index].kind else {
                    continue;
                };
                let (family, size) = (t.font_family, t.font_size);
                let (width, height) = self.editor.measure_block(&text, family, size);
                let fit = |dimension: f64| {
                    let dimension = dimension.ceil() + 2.0 * PADDING;
                    match item.kind.as_str() {
                        "ellipse" => (dimension * std::f64::consts::SQRT_2).round(),
                        "diamond" => 2.0 * dimension,
                        _ => dimension,
                    }
                };
                let shape = &mut self.editor.scene_mut().elements[index];
                shape.base.width = fit(width);
                if !item.json.contains_key("height") {
                    shape.base.height = fit(height);
                }
                // Rounding must not leave the label a hair too wide to fit.
                while max_label_width(&self.editor.scene().elements[index], size) < width {
                    self.editor.scene_mut().elements[index].base.width += 1.0;
                }
            }
            self.editor.layout_text(label_index, &text);
        }
    }

    /// Positions the elements in rows and columns.
    fn place(&mut self, roots: &[Node]) {
        let boxes: HashMap<String, [f64; 2]> = self
            .editor
            .scene()
            .elements
            .iter()
            .map(|e| (e.base.id.clone(), [e.base.width, e.base.height]))
            .collect();
        let mut places = HashMap::new();
        for root in roots {
            if let Node::Group(group) = root {
                layout::place(root, group.at.unwrap_or([0.0, 0.0]), &boxes, &mut places);
            }
        }
        for (id, [x, y]) in places {
            let index = self.index(&id);
            let base = &mut self.editor.scene_mut().elements[index].base;
            (base.x, base.y) = (x, y);
        }
    }

    /// Lays out the labels of the items `which` picks, where their shapes
    /// now are.
    fn lay_out_labels(&mut self, items: &[Item], which: impl Fn(&Item) -> bool) {
        for item in items.iter().filter(|i| i.label.is_some() && which(i)) {
            let label_index = self.index(&item.id) + 1;
            let text = self.editor.scene().elements[label_index]
                .original_text()
                .to_owned();
            self.editor.layout_text(label_index, &text);
        }
    }

    /// Frames take in their children and, without a size of their own,
    /// wrap them with 10 units to spare (`convertToExcalidrawElements`).
    fn fit_frames(&mut self, items: &[Item]) {
        /// Space between a frame and its children.
        const FRAME_PADDING: f64 = 10.0;
        for item in items.iter().filter(|i| i.kind == "frame") {
            let mut children: HashSet<&str> = item.children.iter().map(String::as_str).collect();
            let labels: Vec<String> = items
                .iter()
                .filter(|i| children.contains(i.id.as_str()) && i.label.is_some())
                .map(|i| format!("{}-label", i.id))
                .collect();
            children.extend(labels.iter().map(String::as_str));
            let scene = self.editor.scene_mut();
            for element in scene.elements.iter_mut() {
                if children.contains(element.base.id.as_str()) {
                    element
                        .json_mut()
                        .insert("frameId".into(), item.id.clone().into());
                }
            }
            let sized = ["x", "y", "width", "height"]
                .iter()
                .all(|k| item.json.contains_key(*k));
            if sized || item.children.is_empty() {
                continue;
            }
            let [x1, y1, x2, y2] = crate::edit::common_bounds(
                scene
                    .elements
                    .iter()
                    .filter(|e| children.contains(e.base.id.as_str())),
            );
            let frame = scene
                .elements
                .iter_mut()
                .find(|e| e.base.id == item.id)
                .expect("the frame was made");
            frame.base.x = x1 - FRAME_PADDING;
            frame.base.y = y1 - FRAME_PADDING;
            frame.base.width = x2 - x1 + 2.0 * FRAME_PADDING;
            frame.base.height = y2 - y1 + 2.0 * FRAME_PADDING;
        }
    }

    /// Puts arrows between the shapes they name: from centre to centre
    /// unless they have points, bound on both sides, ends on the outlines.
    fn route_arrows(&mut self, items: &[Item]) -> Result<(), Vec<Issue>> {
        for item in items.iter().filter(|i| i.kind == "arrow") {
            if item.start.is_none() && item.end.is_none() {
                continue;
            }
            let index = self.index(&item.id);
            let centre = |id: &str| {
                let e = &self.editor.scene().elements[self.index(id)].base;
                [e.x + e.width / 2.0, e.y + e.height / 2.0]
            };
            if !item.json.contains_key("points") {
                let (from, to) = (
                    centre(item.start.as_deref().expect("checked in parsing")),
                    centre(item.end.as_deref().expect("checked in parsing")),
                );
                let arrow = &mut self.editor.scene_mut().elements[index];
                (arrow.base.x, arrow.base.y) = (from[0], from[1]);
                if let Kind::Arrow(line) = &mut arrow.kind {
                    line.points = vec![[0.0, 0.0], [to[0] - from[0], to[1] - from[1]]];
                }
                arrow.base.width = (to[0] - from[0]).abs();
                arrow.base.height = (to[1] - from[1]).abs();
            }
            let mut changed = HashSet::new();
            for (end, target) in [(ArrowEnd::Start, &item.start), (ArrowEnd::End, &item.end)] {
                let Some(target) = target else { continue };
                self.editor
                    .set_arrow_binding(index, end, Some(target.clone()));
                // Aimed at the shape's centre, a small gap from its outline.
                let arrow = &mut self.editor.scene_mut().elements[index];
                if let Some(mut binding) = arrow.binding(end) {
                    binding.focus = 0.0;
                    binding.gap = ARROW_GAP;
                    arrow.set_binding(end, Some(binding));
                }
                changed.insert(target.clone());
            }
            self.editor.update_bound_arrows(&changed);
            let arrow = &self.editor.scene().elements[index];
            if let Kind::Arrow(line) = &arrow.kind
                && line.points.len() >= 2
                && line.points.first() == line.points.last()
            {
                return Err(vec![issue(
                    &item.path,
                    "the arrow's shapes overlap, so it has no length; move them apart",
                )]);
            }
        }
        Ok(())
    }
}

fn issue(path: &str, message: &str) -> Issue {
    Issue {
        path: path.to_owned(),
        message: message.to_owned(),
    }
}
