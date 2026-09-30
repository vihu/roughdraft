//! Scene JSON round trips and element bookkeeping.
use super::{Arrowhead, Element, FillStyle, Kind, Scene};

#[test]
fn touch_and_duplicate_update_bookkeeping() {
    let mut scene: Scene = serde_json::from_str(&scene()).unwrap();
    let element = &mut scene.elements[0];
    let before = element.revision();
    element.touch();
    assert_eq!(element.revision().0, before.0 + 1);
    assert_ne!(element.revision().1, before.1);

    let copy = element.duplicate();
    assert_ne!(copy.base.id, element.base.id);
    assert_ne!(copy.base.seed, element.base.seed);
    assert_eq!(copy.revision().0, element.revision().0 + 1);
    let json = serde_json::to_value(&copy).unwrap();
    assert_eq!(json["id"], copy.base.id.as_str());
    assert_eq!(
        json["future"],
        serde_json::json!({"k": [1, 2.5]}),
        "unknown fields kept"
    );
}

const BASE: &str = r##""x":10,"y":20.5,"width":100,"height":50,"angle":0,"strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,"roundness":null,"seed":1968410193,"version":3,"versionNonce":7,"isDeleted":false,"boundElements":null,"updated":1727654400000,"link":null,"locked":false"##;

fn scene() -> String {
    format!(
        r##"{{"type":"excalidraw","version":2,"source":"https://excalidraw.com","elements":[{{"id":"r","type":"rectangle",{BASE},"future":{{"k":[1,2.5]}}}},{{"id":"a","type":"arrow",{BASE},"points":[[0,0],[100,50.25]],"lastCommittedPoint":null,"startBinding":null,"endBinding":null,"startArrowhead":null,"endArrowhead":"arrow","elbowed":false}},{{"id":"t","type":"text",{BASE},"text":"hi\nthere","fontSize":20,"fontFamily":5,"textAlign":"left","verticalAlign":"top","containerId":null,"originalText":"hi there","autoResize":true,"lineHeight":1.25}},{{"id":"f","type":"frame",{BASE},"name":"F1"}}],"appState":{{"gridSize":20,"viewBackgroundColor":"#ffffff"}},"files":{{}}}}"##
    )
}

#[test]
fn round_trip_is_byte_identical() {
    let input = scene();
    let scene: Scene = serde_json::from_str(&input).unwrap();
    assert!(matches!(scene.elements[0].kind, Kind::Rectangle));
    assert!(matches!(&scene.elements[3].kind, Kind::Other(name) if name == "frame"));
    let Kind::Arrow(arrow) = &scene.elements[1].kind else {
        panic!("expected an arrow");
    };
    assert_eq!(arrow.end_arrowhead, Some(Arrowhead::Arrow));
    assert_eq!(serde_json::to_string(&scene).unwrap(), input);
}

#[test]
fn edits_are_written_in_place_as_js_numbers() {
    let mut scene: Scene = serde_json::from_str(&scene()).unwrap();
    scene.elements[0].base.x = 42.0;
    scene.elements[0].base.width = 99.5;
    let out = serde_json::to_string(&scene).unwrap();
    assert!(out.contains(r#""type":"rectangle","x":42,"y":20.5,"width":99.5,"#));
}

#[test]
fn unknown_enum_values_round_trip() {
    let input = scene().replacen(r#""fillStyle":"solid""#, r#""fillStyle":"dots""#, 1);
    let scene: Scene = serde_json::from_str(&input).unwrap();
    assert_eq!(
        scene.elements[0].base.fill_style,
        FillStyle::Other("dots".into())
    );
    assert_eq!(serde_json::to_string(&scene).unwrap(), input);
}

#[test]
fn legacy_fields_take_excalidraw_restore_defaults_and_still_round_trip() {
    let input = r#"{"id":"r","type":"rectangle","x":1,"y":2,"width":30,"height":40,"strokeWidth":0,"strokeSharpness":"round","boundElementIds":["a"]}"#;
    let element: Element = serde_json::from_str(input).unwrap();
    let b = &element.base;
    assert_eq!(
        (b.seed, b.stroke_width, b.opacity, b.roughness, b.angle),
        (1, 2.0, 100.0, 1.0, 0.0)
    );
    assert_eq!(
        (b.stroke_color.as_str(), b.background_color.as_str()),
        ("#1e1e1e", "transparent")
    );
    assert_eq!(
        b.roundness.as_ref().map(|r| r.kind),
        Some(1),
        "legacy radius for rectangles"
    );
    assert_eq!(b.bound_elements.as_ref().unwrap()[0].id, "a");

    let text = r#"{"id":"t","type":"text","x":0,"y":0,"width":50,"height":60,"text":"a\nb","fontSize":20}"#;
    let element: Element = serde_json::from_str(text).unwrap();
    let Kind::Text(text) = &element.kind else {
        panic!("text")
    };
    assert_eq!(
        text.line_height, 1.5,
        "detected from the stored height: 60 / 2 lines / 20"
    );
    assert_eq!(text.font_family, 5);

    // Saving writes the restored fields, like Excalidraw's save.
    let scene = format!(r#"{{"type":"excalidraw","elements":[{input}]}}"#);
    let parsed: Scene = serde_json::from_str(&scene).unwrap();
    assert_eq!(
        serde_json::to_value(&parsed).unwrap()["elements"][0]["seed"],
        1
    );
}

#[test]
fn missing_type_is_rejected() {
    let input = format!(r#"{{"id":"r",{BASE}}}"#);
    let err = serde_json::from_str::<Element>(&input).unwrap_err();
    assert!(err.to_string().contains("missing field `type`"), "{err}");
}

#[test]
fn numbers_parse_exactly_and_write_back_digit_for_digit() {
    // The default serde_json parser reads this as ...542.
    let input = r#"{"type":"excalidraw","elements":[],"n":22.604076400856545}"#;
    let scene: Scene = serde_json::from_str(input).unwrap();
    assert_eq!(serde_json::to_string(&scene).unwrap(), input);
}

#[test]
fn lines_with_fewer_than_two_points_restore_to_their_box() {
    // `restoreElement`: an empty `points` array became a line that panicked
    // when resized.
    let json = format!(
        r#"{{"type":"excalidraw","elements":[{{"id":"l","type":"line",{BASE},"points":[]}}]}}"#
    );
    let scene: Scene = serde_json::from_str(&json).unwrap();
    let Kind::Line(line) = &scene.elements[0].kind else {
        panic!("line")
    };
    assert_eq!(line.points, [[0.0, 0.0], [100.0, 50.0]]);
}

#[test]
fn old_arrows_without_arrowhead_keys_point_at_their_end() {
    let json = format!(
        r#"{{"type":"excalidraw","elements":[{{"id":"a","type":"arrow",{BASE},"points":[[0,0],[100,50]]}}]}}"#
    );
    let scene: Scene = serde_json::from_str(&json).unwrap();
    let Kind::Arrow(line) = &scene.elements[0].kind else {
        panic!("arrow")
    };
    assert_eq!(
        (&line.start_arrowhead, &line.end_arrowhead),
        (&None, &Some(Arrowhead::Arrow))
    );
}

#[test]
fn repeated_ids_get_new_ones_and_null_elements_load_empty() {
    // `restoreElements`: ids stay unique, and `elements: null` is no scene
    // error.
    let json = format!(
        r#"{{"type":"excalidraw","elements":[{{"id":"r","type":"rectangle",{BASE}}},{{"id":"r","type":"rectangle",{BASE}}}]}}"#
    );
    let scene: Scene = serde_json::from_str(&json).unwrap();
    assert_eq!(scene.elements[0].base.id, "r");
    assert_ne!(scene.elements[1].base.id, "r");
    let empty: Scene = serde_json::from_str(r#"{"type":"excalidraw","elements":null}"#).unwrap();
    assert!(empty.elements.is_empty());
}
