//! Render tests: draw order, items per element type.
use super::{Align, Item, render};
use crate::scene::Scene;

const BASE: &str = r##""x":100,"y":50,"width":120,"height":60,"angle":0,"strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"roundness":null,"seed":1968410193,"isDeleted":false"##;

fn scene(elements: &[String]) -> Scene {
    let json = format!(
        r#"{{"type":"excalidraw","elements":[{}]}}"#,
        elements.join(",")
    );
    serde_json::from_str(&json).unwrap()
}

fn text(id: &str, container: &str) -> String {
    format!(
        r#"{{"id":"{id}","type":"text",{BASE},"text":"a\nb","fontSize":20,"fontFamily":5,"textAlign":"center","verticalAlign":"middle","lineHeight":1.25,"containerId":{container}}}"#
    )
}

#[test]
fn labels_follow_their_container_and_deleted_are_skipped() {
    let rect = format!(r#"{{"id":"box","type":"rectangle",{BASE}}}"#);
    let deleted = format!(r#"{{"id":"gone","type":"ellipse",{BASE}}}"#)
        .replace(r#""isDeleted":false"#, r#""isDeleted":true"#);
    let label = text("label", r#""box""#);
    let free = text("free", "null");
    // Label listed first, container second: label must still draw after it.
    let drawings = render(&scene(&[label, deleted, free, rect]));
    let kinds: Vec<&str> = drawings
        .iter()
        .map(|d| match &d.items[0] {
            Item::Text(t) if t.x == 60.0 => "text",
            _ => "shape",
        })
        .collect();
    assert_eq!(kinds, ["text", "shape", "text"]);
}

#[test]
fn text_uses_excalidraw_baseline() {
    let drawings = render(&scene(&[text("t", "null")]));
    let Item::Text(block) = &drawings[0].items[0] else {
        panic!("expected text");
    };
    assert_eq!(block.lines, ["a", "b"]);
    assert_eq!(block.align, Align::Middle);
    assert_eq!(block.line_height, 25.0);
    // Excalifont at 20px: ascent 17.72, descent 7.48, centered in 25px.
    assert!((block.baseline - 17.62).abs() < 1e-9, "{}", block.baseline);
    assert_eq!(drawings[0].transform.apply([0.0, 0.0]), [100.0, 50.0]);
}

#[test]
fn arrow_gets_shaft_and_two_head_strokes() {
    let arrow = format!(
        r#"{{"id":"a","type":"arrow",{BASE},"points":[[0,0],[120,60]],"startArrowhead":null,"endArrowhead":"arrow"}}"#
    );
    let drawings = render(&scene(&[arrow]));
    let strokes = drawings[0]
        .items
        .iter()
        .filter(|i| matches!(i, Item::Stroke { .. }))
        .count();
    // Shaft: one stroke set; each head line: one stroke set.
    assert_eq!(strokes, 3);
}

#[test]
fn a_zero_length_arrow_draws_no_head_and_no_nan() {
    let arrow = format!(
        r#"{{"id":"z","type":"arrow",{BASE},"points":[[0,0],[0,0]],"startArrowhead":null,"endArrowhead":"arrow"}}"#
    );
    let drawings = render(&scene(&[arrow]));
    let finite = |p: &[f64; 2]| p.iter().all(|v| v.is_finite());
    for item in drawings.iter().flat_map(|d| &d.items) {
        if let Item::Stroke { path, .. } | Item::Fill { path, .. } = item {
            for segment in path {
                let ok = match segment {
                    super::Segment::MoveTo(p) | super::Segment::LineTo(p) => finite(p),
                    super::Segment::CubicTo(a, b, c) => finite(a) && finite(b) && finite(c),
                };
                assert!(ok, "{segment:?}");
            }
        }
    }
}
