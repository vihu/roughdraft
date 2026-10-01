//! Problems to fix in a scene before sharing it, in words: what
//! `roughdraft check` reports (PLAN-002). Aimed at files written by hand or
//! by an agent, where the usual faults are guessed text sizes, one-sided
//! bindings and shapes placed on top of each other.
mod layout;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::edit::{Measure, arrow_points, distance_to_outline, label_height_room, max_label_width};
use crate::geometry::{self, Bounds, Point};
use crate::hit::container_id;
use crate::scene::{ArrowEnd, Element, Kind, Scene};

/// How bad a problem is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The file is broken or looks broken: fix it.
    Error,
    /// Probably unintended: look at it.
    Warning,
}

/// One problem, about the elements it names.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    /// What kind of problem, as a short name: `label-overflow`, `overlap`,
    /// and so on.
    pub kind: &'static str,
    /// How bad it is.
    pub severity: Severity,
    /// The elements involved.
    pub ids: Vec<String>,
    /// What is wrong and how to fix it.
    pub message: String,
}

/// Returns a scene file's problems, errors first. `file` is the file's
/// JSON as written: loading repairs some faults (repeated ids, lines
/// without two points) that the file still has. `measure` measures text
/// with the fonts the scene uses.
///
/// # Errors
///
/// Returns the parse error when `file` is not a scene.
pub fn check(file: &Value, measure: &dyn Measure) -> Result<Vec<Problem>, serde_json::Error> {
    let scene: Scene = serde_json::from_value(file.clone())?;
    let mut problems = as_written(file);
    let live: Vec<&Element> = scene
        .elements
        .iter()
        .filter(|e| !e.base.is_deleted)
        .collect();
    let by_id: HashMap<&str, &Element> = live.iter().map(|e| (e.base.id.as_str(), *e)).collect();
    for element in &live {
        sizes(element, &mut problems);
        text(element, &by_id, measure, &mut problems);
        bindings(element, &by_id, &mut problems);
    }
    layout::overlaps(&live, &mut problems);
    layout::crossings(&live, &by_id, measure, &mut problems);
    layout::arrow_labels(&live, &by_id, &mut problems);
    layout::stacked_arrows(&live, &mut problems);
    problems.sort_by_key(|p| p.severity != Severity::Error);
    Ok(problems)
}

/// Font ids roughdraft draws and measures with the right font.
const BUNDLED_FONTS: [u32; 5] = [1, 5, 6, 7, 8];

/// Units a measured text size may differ from the stored one, or this
/// share of it, whichever is larger: fonts shape a hair differently.
const TEXT_SLACK: (f64, f64) = (2.0, 0.03);

/// How far past its binding gap an arrow end may be before it looks
/// detached from its shape.
const END_SLACK: f64 = 10.0;

pub(super) fn problem(
    kind: &'static str,
    severity: Severity,
    elements: &[&Element],
    message: String,
) -> Problem {
    Problem {
        kind,
        severity,
        ids: elements.iter().map(|e| e.base.id.clone()).collect(),
        message,
    }
}

/// Faults in the file as written that loading repairs or drops: repeated
/// ids, and lines and arrows without two points.
fn as_written(file: &Value) -> Vec<Problem> {
    let elements = match file {
        Value::Array(elements) => elements,
        _ => match file.get("elements") {
            Some(Value::Array(elements)) => elements,
            _ => return Vec::new(),
        },
    };
    let mut seen = HashSet::new();
    let mut problems = Vec::new();
    for element in elements
        .iter()
        .filter(|e| e.get("isDeleted") != Some(&Value::Bool(true)))
    {
        let id = element
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let fault = |kind, message: String| Problem {
            kind,
            severity: Severity::Error,
            ids: vec![id.to_owned()],
            message,
        };
        if !seen.insert(id) {
            problems.push(fault(
                "duplicate-id",
                format!("more than one element has the id \"{id}\"; give each its own (Excalidraw renames the repeats, breaking bindings to them)"),
            ));
        }
        let linear = matches!(
            element.get("type").and_then(Value::as_str),
            Some("line" | "arrow")
        );
        let points = element
            .get("points")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        if linear && points < 2 {
            problems.push(fault(
                "zero-size",
                format!("\"{id}\" has fewer than two points; Excalidraw drops or redraws it"),
            ));
        }
    }
    problems
}

/// Elements Excalidraw hides for their size.
fn sizes(element: &Element, problems: &mut Vec<Problem>) {
    let (id, b) = (&element.base.id, &element.base);
    match &element.kind {
        Kind::Text(text) if text.text.trim().is_empty() => problems.push(problem(
            "empty-text",
            Severity::Error,
            &[element],
            format!("text \"{id}\" is empty; remove it or give it words"),
        )),
        // A pen dot is 0 × 0; Excalidraw only drops strokes without points.
        Kind::Line(_) | Kind::Arrow(_) => {}
        Kind::Other(kind) if kind == "freedraw" => {}
        _ if b.width <= 0.0 && b.height <= 0.0 => problems.push(problem(
            "zero-size",
            Severity::Error,
            &[element],
            format!("\"{id}\" is 0 × 0; Excalidraw drops it"),
        )),
        _ if b.width <= 0.0 || b.height <= 0.0 => problems.push(problem(
            "zero-size",
            Severity::Warning,
            &[element],
            format!(
                "\"{id}\" is {:.0} × {:.0}, so it shows as a line; give it both a width and a height",
                b.width, b.height
            ),
        )),
        _ => {}
    }
}

/// Text whose stored size is not its measured size, labels that do not
/// fit their shape, and fonts roughdraft does not have.
fn text(
    element: &Element,
    by_id: &HashMap<&str, &Element>,
    measure: &dyn Measure,
    problems: &mut Vec<Problem>,
) {
    let Kind::Text(text) = &element.kind else {
        return;
    };
    let id = &element.base.id;
    if !BUNDLED_FONTS.contains(&text.font_family) {
        problems.push(problem(
            "font-not-bundled",
            Severity::Warning,
            &[element],
            format!(
                "text \"{id}\" uses fontFamily {}, which roughdraft draws with a stand-in; use 5 (hand-drawn), 6 (normal) or 8 (code)",
                text.font_family
            ),
        ));
        return;
    }
    if text.text.trim().is_empty() {
        return;
    }
    let lines: Vec<&str> = text.text.split('\n').collect();
    let width = lines
        .iter()
        .map(|line| {
            let line = if line.is_empty() { " " } else { line };
            measure.line_width(line, text.font_family, text.font_size)
        })
        .fold(0.0, f64::max);
    let height = lines.len() as f64 * text.font_size * text.line_height;
    let slack = |size: f64| TEXT_SLACK.0.max(size * TEXT_SLACK.1);
    let b = &element.base;
    // Free text with a fixed width wraps to it: only wider is wrong.
    let fixed = text.container_id.is_none() && !element.auto_resize();
    let wrong_width = if fixed {
        width > b.width + slack(width)
    } else {
        (width - b.width).abs() > slack(width)
    };
    if wrong_width || (height - b.height).abs() > slack(height) {
        problems.push(problem(
            "text-size",
            Severity::Warning,
            &[element],
            format!(
                "text \"{id}\" is stored {:.0} × {:.0} but measures {width:.0} × {height:.0} in its font; Excalidraw keeps the stored size, so it will sit off-centre or be cut: set width {width:.0} and height {height:.0}",
                b.width, b.height
            ),
        ));
    }
    let Some(container) = text.container_id.as_deref().and_then(|c| by_id.get(c)) else {
        return;
    };
    if matches!(container.kind, Kind::Arrow(_) | Kind::Line(_)) {
        return;
    }
    let room = max_label_width(container, text.font_size);
    let cid = &container.base.id;
    if width > room + slack(width) {
        problems.push(problem(
            "label-overflow",
            Severity::Error,
            &[container, element],
            format!(
                "the label of \"{cid}\" measures {width:.0} wide but \"{cid}\" has room for {room:.0}: widen \"{cid}\" by {:.0} or break the text into lines",
                width - room
            ),
        ));
    }
    if let Some(needed) = label_height_room(container, height)
        && needed > container.base.height + slack(height)
    {
        problems.push(problem(
            "label-overflow",
            Severity::Error,
            &[container, element],
            format!(
                "the label of \"{cid}\" needs \"{cid}\" to be {needed:.0} high; it is {:.0}",
                container.base.height
            ),
        ));
    }
}

/// Bindings that point nowhere, that only one side knows about, and arrow
/// ends that are far from the shape they are bound to.
fn bindings(element: &Element, by_id: &HashMap<&str, &Element>, problems: &mut Vec<Problem>) {
    let id = &element.base.id;
    if let Kind::Arrow(_) | Kind::Line(_) = element.kind {
        let points = arrow_points(element);
        for (end, name) in [(ArrowEnd::Start, "start"), (ArrowEnd::End, "end")] {
            let Some(binding) = element.binding(end) else {
                continue;
            };
            let target_id = &binding.element_id;
            let Some(target) = by_id.get(target_id.as_str()) else {
                problems.push(problem(
                    "binding-dangling",
                    Severity::Error,
                    &[element],
                    format!("the {name} of \"{id}\" is bound to \"{target_id}\", which does not exist; remove the binding or add the shape"),
                ));
                continue;
            };
            if !lists(target, id) {
                problems.push(problem(
                    "binding-one-sided",
                    Severity::Error,
                    &[element, target],
                    format!("the {name} of \"{id}\" is bound to \"{target_id}\", but \"{target_id}\" does not list it in boundElements; add {{\"id\": \"{id}\", \"type\": \"arrow\"}}"),
                ));
            }
            let Some(&at) = (match end {
                ArrowEnd::Start => points.first(),
                ArrowEnd::End => points.last(),
            }) else {
                continue;
            };
            // An end inside its shape is bound there on purpose.
            let away = distance_to_outline(target, at);
            if !contains(geometry::element_bounds(target), at) && away > binding.gap + END_SLACK {
                problems.push(problem(
                    "arrow-end-off-shape",
                    Severity::Error,
                    &[element, target],
                    format!(
                        "the {name} of \"{id}\" is {away:.0} away from \"{target_id}\", which it is bound to, so it looks detached: move it to the outline of \"{target_id}\""
                    ),
                ));
            }
        }
    }
    if let Some(container) = container_id(element) {
        match by_id.get(container) {
            None => problems.push(problem(
                "binding-dangling",
                Severity::Error,
                &[element],
                format!("text \"{id}\" belongs to \"{container}\", which does not exist; clear its containerId"),
            )),
            Some(owner) if !lists(owner, id) => problems.push(problem(
                "binding-one-sided",
                Severity::Error,
                &[element, owner],
                format!("text \"{id}\" belongs to \"{container}\", but \"{container}\" does not list it in boundElements; add {{\"id\": \"{id}\", \"type\": \"text\"}}"),
            )),
            Some(_) => {}
        }
    }
    // Excalidraw leaves such entries behind, drops the ones for missing or
    // deleted elements on load and ignores the rest.
    for bound in element.base.bound_elements.iter().flatten() {
        let other = &bound.id;
        let Some(target) = by_id.get(other.as_str()) else {
            problems.push(problem(
                "binding-stale",
                Severity::Warning,
                &[element],
                format!("\"{id}\" lists \"{other}\" in boundElements, which does not exist; remove the entry"),
            ));
            continue;
        };
        let knows = match target.kind {
            Kind::Text(_) => container_id(target) == Some(id.as_str()),
            _ => [ArrowEnd::Start, ArrowEnd::End]
                .iter()
                .any(|&end| target.binding(end).is_some_and(|b| &b.element_id == id)),
        };
        if !knows {
            problems.push(problem(
                "binding-stale",
                Severity::Warning,
                &[element, target],
                format!("\"{id}\" lists \"{other}\" in boundElements, but \"{other}\" is not attached to it; remove the entry, or attach it if it should follow \"{id}\""),
            ));
        }
    }
}

/// Whether `element` lists `id` in its `boundElements`.
fn lists(element: &Element, id: &str) -> bool {
    element
        .base
        .bound_elements
        .iter()
        .flatten()
        .any(|b| b.id == id)
}

pub(super) fn contains([x1, y1, x2, y2]: Bounds, [x, y]: Point) -> bool {
    (x1..=x2).contains(&x) && (y1..=y2).contains(&y)
}
