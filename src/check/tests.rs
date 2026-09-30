//! Each kind of problem, and a clean diagram with none, measured with
//! [`ApproxMeasure`] (every character 0.55 em: "Hello" at 20 is 55 wide).
use serde_json::{Value, json};

use super::{Problem, Severity, check};
use crate::edit::ApproxMeasure;

/// Fields every element needs, merged under `fields`.
fn element(fields: Value) -> Value {
    let mut base = json!({
        "angle": 0, "strokeColor": "#1e1e1e", "backgroundColor": "transparent",
        "fillStyle": "solid", "strokeWidth": 2, "strokeStyle": "solid",
        "roughness": 1, "opacity": 100, "seed": 1, "version": 1, "versionNonce": 1,
        "isDeleted": false, "groupIds": [], "boundElements": null,
    });
    for (key, value) in fields.as_object().unwrap() {
        base[key] = value.clone();
    }
    base
}

fn rect(id: &str, x: f64, y: f64, w: f64, h: f64) -> Value {
    element(json!({"id": id, "type": "rectangle", "x": x, "y": y, "width": w, "height": h}))
}

fn problems(elements: Vec<Value>) -> Vec<Problem> {
    check(
        &json!({"type": "excalidraw", "elements": elements}),
        &ApproxMeasure,
    )
    .unwrap()
}

fn kinds(found: &[Problem]) -> Vec<&'static str> {
    found.iter().map(|p| p.kind).collect()
}

#[test]
fn a_built_diagram_has_no_problems() {
    let built = crate::build::build(
        &json!([
            {"type": "row", "x": 0, "y": 0, "gap": 120, "children": [
                {"type": "rectangle", "id": "a", "width": 160, "height": 70, "label": "Browser"},
                {"type": "ellipse", "id": "b", "label": "API"},
                {"type": "diamond", "id": "c", "label": "Ok?"},
            ]},
            {"type": "text", "id": "t", "x": 0, "y": -60, "text": "Title"},
            {"type": "arrow", "id": "r1", "start": {"id": "a"}, "end": {"id": "b"}, "label": "HTTP"},
            {"type": "arrow", "id": "r2", "start": {"id": "b"}, "end": {"id": "c"}},
        ]),
        Box::new(ApproxMeasure),
    )
    .unwrap();
    let file = serde_json::to_value(&built.scene).unwrap();
    assert_eq!(check(&file, &ApproxMeasure).unwrap(), []);
}

#[test]
fn duplicate_ids_and_elements_too_small_to_show() {
    let found = problems(vec![
        rect("a", 0.0, 0.0, 50.0, 50.0),
        rect("a", 100.0, 0.0, 50.0, 50.0),
        rect("flat", 200.0, 0.0, 50.0, 0.0),
        element(
            json!({"id": "l", "type": "line", "x": 0, "y": 100, "width": 0, "height": 0, "points": [[0, 0]]}),
        ),
        element(
            json!({"id": "t", "type": "text", "x": 0, "y": 200, "width": 0, "height": 25, "text": " ", "fontSize": 20, "fontFamily": 5, "lineHeight": 1.25}),
        ),
    ]);
    assert_eq!(
        kinds(&found),
        ["duplicate-id", "zero-size", "empty-text", "zero-size"]
    );
    // 50 × 0 still shows, as a line.
    assert_eq!(found[3].severity, Severity::Warning);
    assert_eq!(found[3].ids, ["flat"]);
}

#[test]
fn text_stored_at_the_wrong_size_and_labels_that_do_not_fit() {
    let text = |id: &str, x: f64, w: f64, text: &str, container: Option<&str>, font: u32| {
        element(
            json!({"id": id, "type": "text", "x": x, "y": 0, "width": w, "height": 25,
            "text": text, "fontSize": 20, "fontFamily": font, "lineHeight": 1.25, "containerId": container}),
        )
    };
    let mut boxed = rect("box", 0.0, 100.0, 40.0, 30.0);
    boxed["boundElements"] = json!([{"id": "label", "type": "text"}]);
    let found = problems(vec![
        // "Hello" measures 55: stored as 20.
        text("guess", 0.0, 20.0, "Hello", None, 5),
        text("right", 100.0, 55.0, "Hello", None, 5),
        text("cascadia", 200.0, 55.0, "Hello", None, 3),
        boxed,
        text("label", 5.0, 55.0, "Hello", Some("box"), 5),
    ]);
    assert_eq!(
        kinds(&found),
        [
            "label-overflow",
            "label-overflow",
            "text-size",
            "font-not-bundled"
        ],
        "{found:#?}"
    );
    // The box has room for 30 across (40 less padding) and needs 35 high.
    assert!(
        found[0].message.contains("has room for 30"),
        "{}",
        found[0].message
    );
    assert!(
        found[1].message.contains("to be 35 high"),
        "{}",
        found[1].message
    );
}

#[test]
fn bindings_that_point_nowhere_or_are_known_to_one_side_only() {
    let arrow = |id: &str, end: &str| {
        element(
            json!({"id": id, "type": "arrow", "x": 100, "y": 25, "width": 100, "height": 0,
            "points": [[0, 0], [100, 0]], "endBinding": {"elementId": end, "focus": 0, "gap": 5}}),
        )
    };
    let mut lists_ghost = rect("b", 400.0, 0.0, 50.0, 50.0);
    lists_ghost["boundElements"] = json!([{"id": "ghost", "type": "arrow"}]);
    let found = problems(vec![
        // `a` is where the arrow ends but does not list it.
        rect("a", 205.0, 0.0, 50.0, 50.0),
        arrow("to-a", "a"),
        arrow("to-nothing", "nope"),
        lists_ghost,
    ]);
    assert_eq!(
        kinds(&found),
        ["binding-one-sided", "binding-dangling", "binding-stale"],
        "{found:#?}"
    );
}

#[test]
fn an_arrow_bound_to_a_shape_far_from_its_end_looks_detached() {
    let mut shape = rect("a", 400.0, 0.0, 50.0, 50.0);
    shape["boundElements"] = json!([{"id": "r", "type": "arrow"}]);
    let found = problems(vec![
        shape,
        element(
            json!({"id": "r", "type": "arrow", "x": 0, "y": 25, "width": 100, "height": 0,
            "points": [[0, 0], [100, 0]], "endBinding": {"elementId": "a", "focus": 0, "gap": 5}}),
        ),
    ]);
    assert_eq!(kinds(&found), ["arrow-end-off-shape"]);
    assert!(
        found[0].message.contains("300 away"),
        "{}",
        found[0].message
    );
}

#[test]
fn shapes_on_top_of_each_other_and_arrows_over_shapes() {
    let mut grouped = [
        rect("g1", 0.0, 300.0, 50.0, 50.0),
        rect("g2", 30.0, 300.0, 50.0, 50.0),
    ];
    for g in &mut grouped {
        g["groupIds"] = json!(["icon"]);
    }
    let [g1, g2] = grouped;
    let found = problems(vec![
        rect("a", 0.0, 0.0, 100.0, 100.0),
        rect("b", 80.0, 20.0, 100.0, 100.0),
        // Wholly inside `a`: a panel and what is on it.
        rect("inner", 10.0, 10.0, 20.0, 20.0),
        g1,
        g2,
        rect("wall", 300.0, 150.0, 20.0, 100.0),
        element(
            json!({"id": "r", "type": "arrow", "x": 200, "y": 200, "width": 200, "height": 0,
            "points": [[0, 0], [200, 0]]}),
        ),
    ]);
    assert_eq!(kinds(&found), ["overlap", "arrow-crosses"], "{found:#?}");
    assert_eq!(found[0].ids, ["a", "b"]);
    assert!(found[0].message.contains("20 × 80"), "{}", found[0].message);
    assert_eq!(found[1].ids, ["r", "wall"]);
}
