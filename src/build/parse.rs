//! Reading a skeleton: items flattened out of their rows and columns,
//! checked, with every problem found named by where it is.
use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use super::layout::{Align, Axis, Group, Node};
use super::{Issue, issue};

/// Default gap between the children of a row or column.
const GAP: f64 = 40.0;

/// Font ids roughdraft draws with the right font (Virgil, Excalifont,
/// Nunito, Lilita One, Comic Shanns).
const BUNDLED_FONTS: [u64; 5] = [1, 5, 6, 7, 8];

/// One element of the input, flattened out of its groups.
#[derive(Debug)]
pub(super) struct Item {
    pub(super) path: String,
    pub(super) id: String,
    pub(super) kind: String,
    pub(super) json: Map<String, Value>,
    pub(super) label: Option<Map<String, Value>>,
    pub(super) start: Option<String>,
    pub(super) end: Option<String>,
    pub(super) children: Vec<String>,
}

/// The input read so far: its elements, and what is wrong with it.
#[derive(Default)]
pub(super) struct Parse {
    pub(super) items: Vec<Item>,
    ids: HashSet<String>,
    pub(super) errors: Vec<Issue>,
    pub(super) warnings: Vec<Issue>,
}

impl Parse {
    /// Reads one item, and a group's children; returns its layout node.
    pub(super) fn item(&mut self, value: &Value, path: &str, in_group: bool) -> Option<Node> {
        let Some(json) = value.as_object() else {
            self.errors.push(issue(path, "expected an object"));
            return None;
        };
        let Some(kind) = json.get("type").and_then(Value::as_str) else {
            self.errors.push(issue(path, "missing \"type\""));
            return None;
        };
        match kind {
            "row" | "column" => self.group(json, kind, path, in_group),
            "rectangle" | "ellipse" | "diamond" | "text" | "arrow" | "line" | "frame" => {
                self.element(json, kind, path, in_group)
            }
            other => {
                self.errors.push(issue(
                    &format!("{path}.type"),
                    &format!(
                        "unknown type \"{other}\"; use rectangle, ellipse, diamond, text, arrow, line, frame, row or column"
                    ),
                ));
                None
            }
        }
    }

    fn group(
        &mut self,
        json: &Map<String, Value>,
        kind: &str,
        path: &str,
        in_group: bool,
    ) -> Option<Node> {
        let Some(children) = json.get("children").and_then(Value::as_array) else {
            self.errors
                .push(issue(path, "a row or column needs a \"children\" array"));
            return None;
        };
        let number = |key: &str| json.get(key).and_then(Value::as_f64);
        let (x, y) = (number("x"), number("y"));
        if in_group && (x.is_some() || y.is_some()) {
            self.warn(path, "x/y ignored: the enclosing row or column places it");
        }
        let at = (!in_group).then(|| [x.unwrap_or(0.0), y.unwrap_or(0.0)]);
        let align = match json.get("align").and_then(Value::as_str) {
            None | Some("center") => Align::Center,
            Some("start") => Align::Start,
            Some("end") => Align::End,
            Some(other) => {
                self.errors.push(issue(
                    &format!("{path}.align"),
                    &format!("unknown align \"{other}\"; use start, center or end"),
                ));
                Align::Center
            }
        };
        let children = children
            .iter()
            .enumerate()
            .filter_map(|(i, child)| self.item(child, &format!("{path}.children[{i}]"), true))
            .collect();
        Some(Node::Group(Group {
            axis: if kind == "row" {
                Axis::Row
            } else {
                Axis::Column
            },
            gap: number("gap").unwrap_or(GAP),
            align,
            at,
            children,
        }))
    }

    fn element(
        &mut self,
        json: &Map<String, Value>,
        kind: &str,
        path: &str,
        in_group: bool,
    ) -> Option<Node> {
        let id = match json.get("id") {
            Some(Value::String(id)) => id.clone(),
            None => crate::random::id(),
            Some(_) => {
                self.errors
                    .push(issue(&format!("{path}.id"), "an id must be a string"));
                return None;
            }
        };
        if !self.ids.insert(id.clone()) {
            self.errors.push(issue(
                &format!("{path}.id"),
                &format!("duplicate id \"{id}\""),
            ));
            return None;
        }
        let linear = matches!(kind, "arrow" | "line");
        if linear && in_group {
            self.errors.push(issue(
                path,
                "arrows and lines cannot be in a row or column; list them outside and connect shapes with start/end",
            ));
            return None;
        }
        let has_position = json.contains_key("x") || json.contains_key("y");
        if in_group && has_position {
            self.warn(path, "x/y ignored: the enclosing row or column places it");
        }
        let label = match json.get("label") {
            None => None,
            Some(Value::String(text)) => {
                Some(Map::from_iter([("text".into(), text.clone().into())]))
            }
            Some(Value::Object(label)) if label.get("text").is_some_and(Value::is_string) => {
                Some(label.clone())
            }
            Some(_) => {
                self.errors.push(issue(
                    &format!("{path}.label"),
                    "a label is a string or {\"text\": ...}",
                ));
                return None;
            }
        };
        if label.is_some() && matches!(kind, "text" | "line" | "frame") {
            self.errors.push(issue(
                &format!("{path}.label"),
                &format!("a {kind} cannot have a label; labels go on rectangles, ellipses, diamonds and arrows"),
            ));
            return None;
        }
        if kind == "text"
            && !json
                .get("text")
                .is_some_and(|t| t.as_str().is_some_and(|t| !t.is_empty()))
        {
            self.errors
                .push(issue(path, "a text item needs a non-empty \"text\""));
            return None;
        }
        let end = |key: &str, this: &mut Self| -> Result<Option<String>, ()> {
            match json.get(key) {
                None => Ok(None),
                Some(end) => match end.get("id").and_then(Value::as_str) {
                    Some(id) => Ok(Some(id.to_owned())),
                    None => {
                        this.errors.push(issue(
                            &format!("{path}.{key}"),
                            "use {\"id\": \"...\"} naming another item; shapes inside start/end are not supported",
                        ));
                        Err(())
                    }
                },
            }
        };
        let (Ok(start), Ok(end)) = (end("start", self), end("end", self)) else {
            return None;
        };
        if (start.is_some() || end.is_some()) && kind != "arrow" {
            self.errors.push(issue(
                path,
                "only arrows connect to shapes; for a plain connector use an arrow with \"endArrowhead\": null",
            ));
            return None;
        }
        if kind == "arrow" && !json.contains_key("points") && (start.is_none() || end.is_none()) {
            self.errors.push(issue(
                path,
                "an arrow needs both \"start\" and \"end\", or \"points\"",
            ));
            return None;
        }
        // A frame with children is fitted around them.
        let fitted = kind == "frame" && json.get("children").is_some();
        if !in_group && !has_position && !linear && !fitted {
            self.warn(path, "no x/y; placed at 0, 0");
        }
        let children = json
            .get("children")
            .and_then(Value::as_array)
            .map(|c| {
                c.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        if kind != "text" {
            for key in ["fontFamily", "fontSize"]
                .iter()
                .filter(|k| json.contains_key(**k))
            {
                self.warn(
                    &format!("{path}.{key}"),
                    &format!("{key} does nothing on a {kind}; put it in the label: {{\"label\": {{\"text\": ..., \"{key}\": ...}}}}"),
                );
            }
        }
        for font in [
            json.get("fontFamily"),
            label.as_ref().and_then(|l| l.get("fontFamily")),
        ]
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        {
            if !BUNDLED_FONTS.contains(&font) {
                self.warn(
                    path,
                    &format!("fontFamily {font} is not bundled; use 5 (hand-drawn), 6 (normal) or 8 (code)"),
                );
            }
        }
        self.items.push(Item {
            path: path.to_owned(),
            id: id.clone(),
            kind: kind.to_owned(),
            json: json.clone(),
            label,
            start,
            end,
            children,
        });
        Some(Node::Element(id))
    }

    /// Arrow ends and frame children must name items that exist.
    pub(super) fn check_references(&mut self) {
        let kinds: HashMap<&str, &str> = self
            .items
            .iter()
            .map(|i| (i.id.as_str(), i.kind.as_str()))
            .collect();
        let mut errors = Vec::new();
        for item in &self.items {
            for (key, target) in [("start", &item.start), ("end", &item.end)] {
                let Some(target) = target else { continue };
                match kinds.get(target.as_str()) {
                    None => errors.push(issue(
                        &format!("{}.{key}.id", item.path),
                        &format!("no item with id \"{target}\""),
                    )),
                    Some(kind)
                        if !matches!(*kind, "rectangle" | "ellipse" | "diamond" | "text") =>
                    {
                        errors.push(issue(
                            &format!("{}.{key}.id", item.path),
                            &format!("an arrow can end on a rectangle, ellipse, diamond or text, not a {kind}"),
                        ));
                    }
                    Some(_) => {}
                }
            }
            let label_id = format!("{}-label", item.id);
            if item.label.is_some() && kinds.contains_key(label_id.as_str()) {
                errors.push(issue(
                    &item.path,
                    &format!("its label would take the id \"{label_id}\", which another item has"),
                ));
            }
            if item.start.is_some() && item.start == item.end && !item.json.contains_key("points") {
                errors.push(issue(
                    &item.path,
                    "an arrow from a shape to itself needs \"points\" to go around",
                ));
            }
            for child in &item.children {
                if !kinds.contains_key(child.as_str()) {
                    errors.push(issue(
                        &format!("{}.children", item.path),
                        &format!("no item with id \"{child}\""),
                    ));
                }
            }
        }
        self.errors.extend(errors);
    }

    fn warn(&mut self, path: &str, message: &str) {
        self.warnings.push(issue(path, message));
    }
}
