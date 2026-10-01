//! Building scenes from skeletons (PLAN-002), measured with
//! [`ApproxMeasure`]: every character 0.55 em wide, so a 20 px character is
//! 11 units and a line is 25 high.
use serde_json::{Value, json};

use super::{Built, Issue, build};
use crate::edit::ApproxMeasure;
use crate::scene::{ArrowEnd, Element, Kind};

fn built(input: Value) -> Built {
    build(&input, Box::new(ApproxMeasure)).unwrap_or_else(|e| panic!("{e:?}"))
}

fn errors(input: Value) -> Vec<Issue> {
    build(&input, Box::new(ApproxMeasure)).expect_err("should fail")
}

fn element<'a>(built: &'a Built, id: &str) -> &'a Element {
    built
        .scene
        .elements
        .iter()
        .find(|e| e.base.id == id)
        .unwrap_or_else(|| panic!("no {id}"))
}

fn rect(built: &Built, id: &str) -> (f64, f64, f64, f64) {
    let b = &element(built, id).base;
    (b.x, b.y, b.width, b.height)
}

#[test]
fn a_label_sizes_its_shape_and_both_sides_are_bound() {
    let built = built(json!([
        {"type": "rectangle", "id": "a", "x": 0, "y": 0, "label": "Hello"},
    ]));
    // "Hello": 55 by 25, plus 5 of padding on each side.
    assert_eq!(rect(&built, "a"), (0.0, 0.0, 65.0, 35.0));
    assert_eq!(rect(&built, "a-label"), (5.0, 5.0, 55.0, 25.0));
    let label = element(&built, "a-label");
    let Kind::Text(text) = &label.kind else {
        panic!("a text label")
    };
    assert_eq!(text.container_id.as_deref(), Some("a"));
    let bound = element(&built, "a").base.bound_elements.clone().unwrap();
    assert_eq!(bound[0].id, "a-label");
}

#[test]
fn a_long_label_wraps_to_its_shape_which_grows_taller() {
    let built = built(json!([
        {"type": "rectangle", "id": "a", "x": 0, "y": 0, "width": 100, "height": 40,
         "label": {"text": "one two three four"}},
    ]));
    let Kind::Text(text) = &element(&built, "a-label").kind else {
        panic!("a text label")
    };
    let lines = text.text.lines().count();
    assert!(lines > 1, "wrapped: {:?}", text.text);
    let (_, _, width, height) = rect(&built, "a");
    assert_eq!(width, 100.0);
    assert_eq!(height, lines as f64 * 25.0 + 10.0);
}

#[test]
fn free_text_is_measured() {
    let built = built(json!([{"type": "text", "id": "t", "x": 10, "y": 20, "text": "Hi\nthere"}]));
    assert_eq!(rect(&built, "t"), (10.0, 20.0, 55.0, 50.0));
}

#[test]
fn an_arrow_between_shapes_ends_on_their_outlines_bound_both_ways() {
    let built = built(json!([
        {"type": "rectangle", "id": "a", "x": 0, "y": 0, "width": 100, "height": 50},
        {"type": "rectangle", "id": "b", "x": 300, "y": 0, "width": 100, "height": 50},
        {"type": "arrow", "id": "r", "start": {"id": "a"}, "end": {"id": "b"}},
    ]));
    let arrow = element(&built, "r");
    let Kind::Arrow(line) = &arrow.kind else {
        panic!("an arrow")
    };
    let start = [arrow.base.x, arrow.base.y];
    let end = [start[0] + line.points[1][0], start[1] + line.points[1][1]];
    // 6 units off each outline, on the line between the centres.
    assert!(
        (start[0] - 106.0).abs() < 0.5 && (start[1] - 25.0).abs() < 0.5,
        "{start:?}"
    );
    assert!(
        (end[0] - 294.0).abs() < 0.5 && (end[1] - 25.0).abs() < 0.5,
        "{end:?}"
    );
    for (end, shape) in [(ArrowEnd::Start, "a"), (ArrowEnd::End, "b")] {
        let binding = arrow.binding(end).expect("bound");
        assert_eq!(
            (binding.element_id.as_str(), binding.focus, binding.gap),
            (shape, 0.0, 6.0)
        );
        let bound = element(&built, shape).base.bound_elements.clone().unwrap();
        assert!(bound.iter().any(|b| b.id == "r" && b.kind == "arrow"));
    }
}

#[test]
fn rows_and_columns_place_and_align_their_children() {
    let built = built(json!([
        {"type": "column", "x": 10, "y": 10, "gap": 20, "align": "start", "children": [
            {"type": "row", "gap": 10, "children": [
                {"type": "rectangle", "id": "big", "width": 50, "height": 50},
                {"type": "ellipse", "id": "small", "width": 30, "height": 30},
            ]},
            {"type": "text", "id": "note", "text": "Hi"},
        ]},
        {"type": "row", "x": 200, "y": 0, "align": "end", "children": [
            {"type": "rectangle", "id": "tall", "width": 20, "height": 80},
            {"type": "rectangle", "id": "short", "width": 20, "height": 20},
        ]},
    ]));
    assert_eq!(rect(&built, "big"), (10.0, 10.0, 50.0, 50.0));
    // Centred across the row by default.
    assert_eq!(rect(&built, "small"), (70.0, 20.0, 30.0, 30.0));
    assert_eq!(rect(&built, "note"), (10.0, 80.0, 22.0, 25.0));
    // The default gap is 40; `end` lines them up at the bottom.
    assert_eq!(rect(&built, "short"), (260.0, 60.0, 20.0, 20.0));
}

#[test]
fn a_frame_takes_in_its_children() {
    let built = built(json!([
        {"type": "rectangle", "id": "a", "x": 0, "y": 0, "width": 100, "height": 50, "label": "A"},
        {"type": "frame", "id": "f", "name": "Box", "children": ["a"]},
    ]));
    assert_eq!(rect(&built, "f"), (-10.0, -10.0, 120.0, 70.0));
    for id in ["a", "a-label"] {
        assert_eq!(element(&built, id).frame_id(), Some("f"), "{id}");
    }
}

#[test]
fn the_output_has_every_field_and_reads_back_the_same() {
    let built = built(json!({
        "elements": [
            {"type": "diamond", "id": "d", "x": 0, "y": 0, "label": "Ok?", "backgroundColor": "#ffec99"},
            {"type": "ellipse", "id": "e", "x": 200, "y": 0},
            {"type": "arrow", "id": "r", "start": {"id": "d"}, "end": {"id": "e"}, "label": "yes"},
        ],
        "appState": {"viewBackgroundColor": "#f8f9fa"},
    }));
    let text = built.scene.to_json();
    let json: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["appState"]["viewBackgroundColor"], "#f8f9fa");
    for element in json["elements"].as_array().unwrap() {
        for key in [
            "id",
            "type",
            "x",
            "y",
            "width",
            "height",
            "seed",
            "version",
            "versionNonce",
            "groupIds",
            "boundElements",
            "roundness",
        ] {
            assert!(element.get(key).is_some(), "{} lacks {key}", element["id"]);
        }
        assert!(
            element["width"].as_f64().unwrap() > 0.0 || element["height"].as_f64().unwrap() > 0.0
        );
    }
    assert_eq!(json["elements"][0]["backgroundColor"], "#ffec99");
    let again: crate::scene::Scene = serde_json::from_str(&text).unwrap();
    assert_eq!(again.to_json(), text);
}

#[test]
fn problems_name_where_they_are() {
    let found = errors(json!([
        {"type": "rectangle", "id": "a", "x": 0, "y": 0},
        {"type": "rectangle", "id": "a", "x": 0, "y": 0},
        {"type": "star", "x": 0, "y": 0},
        {"type": "arrow", "start": {"id": "a"}, "end": {"id": "nope"}},
        {"type": "arrow", "start": {"type": "rectangle"}, "end": {"id": "a"}},
        {"type": "row", "children": [{"type": "arrow", "points": [[0, 0], [10, 0]]}]},
        {"type": "text", "x": 0, "y": 0, "text": ""},
    ]));
    let paths: Vec<&str> = found.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "elements[1].id",
            "elements[2].type",
            "elements[4].start",
            "elements[5].children[0]",
            "elements[6]",
            "elements[3].end.id",
        ],
        "{found:#?}"
    );
}

#[test]
fn ignored_positions_and_unbundled_fonts_are_warned_about() {
    let built = built(json!([
        {"type": "row", "children": [{"type": "text", "x": 5, "y": 5, "text": "a", "fontFamily": 3}]},
        {"type": "rectangle", "x": 0, "y": 100, "label": "b", "fontFamily": 6},
    ]));
    let messages: Vec<&str> = built.warnings.iter().map(|w| w.message.as_str()).collect();
    assert_eq!(messages.len(), 3, "{messages:?}");
    assert!(messages[0].contains("x/y ignored"));
    assert!(messages[1].contains("fontFamily 3"));
    assert!(messages[2].contains("put it in the label"));
}

#[test]
fn a_label_that_outgrows_its_shape_is_warned_about() {
    // "Hello there" measures 121 at 20 and the box has room for 90.
    let built = built(json!([
        {"type": "rectangle", "x": 0, "y": 0, "width": 100, "height": 40, "label": "Hello there\nfriend"},
    ]));
    let paths: Vec<&str> = built.warnings.iter().map(|w| w.path.as_str()).collect();
    assert_eq!(
        paths,
        ["elements[0].height", "elements[0].label"],
        "{:?}",
        built.warnings
    );
    assert!(built.warnings[0].message.contains("grew from 40"));
    assert!(
        built.warnings[1]
            .message
            .contains("the 90 the shape has room for"),
        "{}",
        built.warnings[1].message
    );
}
