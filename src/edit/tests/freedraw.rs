//! Drawing with the pen (REFERENCE-001 section 16).
use crate::edit::{Command, Editor, Modifiers, Pointer, Tool};
use crate::scene::Scene;

fn pen() -> Editor {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Freedraw));
    editor
}

/// Presses at the first point, moves through the rest and releases on the
/// last.
fn stroke(editor: &mut Editor, path: &[[f64; 2]]) {
    let none = Modifiers::default();
    editor.pointer(Pointer::Down, path[0], none);
    for &at in &path[1..] {
        editor.pointer(Pointer::Move, at, none);
    }
    editor.pointer(Pointer::Up, path[path.len() - 1], none);
}

fn json(editor: &Editor) -> serde_json::Value {
    serde_json::to_value(editor.scene()).unwrap()["elements"][0].clone()
}

#[test]
fn a_stroke_follows_the_pointer_and_the_pen_stays() {
    let mut editor = pen();
    // The repeated point is dropped; the release adds the last one again,
    // as Excalidraw does.
    stroke(
        &mut editor,
        &[[10.0, 10.0], [20.0, 15.0], [20.0, 15.0], [40.0, 30.0]],
    );
    let pen = json(&editor);
    assert_eq!(pen["type"], "freedraw");
    assert_eq!(
        (pen["x"].as_f64(), pen["y"].as_f64()),
        (Some(10.0), Some(10.0))
    );
    assert_eq!(
        pen["points"],
        serde_json::json!([[0.0, 0.0], [10.0, 5.0], [30.0, 20.0], [30.0, 20.0]])
    );
    assert_eq!(
        (pen["width"].as_f64(), pen["height"].as_f64()),
        (Some(30.0), Some(20.0))
    );
    assert_eq!(pen["pressures"], serde_json::json!([]));
    assert_eq!(pen["simulatePressure"], true);
    assert_eq!(pen["lastCommittedPoint"], serde_json::json!([30.0, 20.0]));
    assert_eq!(pen["roundness"], serde_json::Value::Null);
    // Nothing selected, and the pen stays without the tool lock.
    assert_eq!(editor.selection().count(), 0);
    assert_eq!(editor.tool(), Tool::Freedraw);
    stroke(&mut editor, &[[100.0, 100.0], [120.0, 100.0]]);
    assert_eq!(editor.scene().elements.len(), 2);
    editor.command(Command::Undo);
    assert_eq!(editor.scene().elements.len(), 1);
}

#[test]
fn a_click_leaves_a_dot() {
    let mut editor = pen();
    stroke(&mut editor, &[[5.0, 5.0]]);
    assert_eq!(
        json(&editor)["points"],
        serde_json::json!([[0.0, 0.0], [0.0001, 0.0001]])
    );
}

#[test]
fn a_stroke_ending_near_its_start_closes_the_loop() {
    let mut editor = pen();
    // Ends 5 units from the start: within 8 screen px at zoom 1.
    stroke(
        &mut editor,
        &[[0.0, 0.0], [100.0, 0.0], [50.0, 80.0], [3.0, 4.0]],
    );
    let points = json(&editor)["points"].clone();
    assert_eq!(points[4], serde_json::json!([0.0, 0.0]));
    // Zoomed in 2x, 8 screen px is 4 units: the same stroke stays open.
    let mut editor = pen();
    editor.set_zoom(2.0);
    stroke(
        &mut editor,
        &[[0.0, 0.0], [100.0, 0.0], [50.0, 80.0], [3.0, 4.0]],
    );
    assert_eq!(json(&editor)["points"][4], serde_json::json!([3.0, 4.0]));
}

#[test]
fn a_command_mid_stroke_drops_the_stroke() {
    let mut editor = pen();
    editor.pointer(Pointer::Down, [0.0, 0.0], Modifiers::default());
    editor.pointer(Pointer::Move, [30.0, 30.0], Modifiers::default());
    assert_eq!(editor.active().len(), 1);
    editor.command(Command::Escape);
    assert!(editor.scene().elements.is_empty());
}
