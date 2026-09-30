//! Copy, cut and paste.
use super::{Modifiers, drag, editor};
use crate::edit::Command;
use crate::scene::Kind;

fn select_b(editor: &mut crate::edit::Editor) {
    drag(editor, [250.0, 25.0], [250.0, 25.0], Modifiers::default());
}

#[test]
fn copy_writes_excalidraw_clipboard_json_with_labels() {
    let mut editor = editor();
    select_b(&mut editor);
    let json: serde_json::Value = serde_json::from_str(&editor.copy().unwrap()).unwrap();
    assert_eq!(json["type"], "excalidraw/clipboard");
    let ids: Vec<&str> = json["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["b", "t"]);
    assert!(editor.copy().is_some());
    editor.command(Command::Escape);
}

#[test]
fn paste_centres_on_the_cursor_with_fresh_ids_and_undoes() {
    let mut editor = editor();
    select_b(&mut editor);
    let json = editor.copy().unwrap();
    assert!(editor.paste(&json, [500.0, 500.0]));
    let elements = &editor.scene().elements;
    let (copy, label) = (&elements[4], &elements[5]);
    // Content box is b plus its wider label: x 200..310, y 0..50.
    assert_eq!((copy.base.x, copy.base.y), (445.0, 475.0));
    assert_ne!(copy.base.id, "b");
    assert_ne!(copy.base.seed, elements[1].base.seed);
    let Kind::Text(text) = &label.kind else {
        panic!("label")
    };
    assert_eq!(text.container_id.as_deref(), Some(copy.base.id.as_str()));
    assert!(editor.is_selected(&copy.base.id) && !editor.is_selected(&label.base.id));
    editor.command(Command::Undo);
    assert_eq!(editor.scene().elements.len(), 4);
}

#[test]
fn pasted_arrow_without_its_shape_is_unbound_and_cut_deletes() {
    let mut editor = editor();
    drag(
        &mut editor,
        [150.0, 0.0],
        [150.0, 0.0],
        Modifiers::default(),
    );
    assert!(editor.is_selected("r"));
    let json = editor.cut().unwrap();
    assert!(editor.scene().elements[3].base.is_deleted);
    assert!(editor.paste(&json, [0.0, 300.0]));
    let pasted = serde_json::to_value(editor.scene().elements.last().unwrap()).unwrap();
    assert!(pasted["endBinding"].is_null());
}

#[test]
fn paste_accepts_scene_files_and_refuses_other_text() {
    let mut editor = editor();
    assert!(!editor.paste("hello", [0.0, 0.0]));
    assert!(!editor.paste(r#"{"type":"other","elements":[]}"#, [0.0, 0.0]));
    let scene = std::fs::read_to_string("tests/fixtures/scenes/l0-coverage.excalidraw").unwrap();
    assert!(editor.paste(&scene, [0.0, 0.0]));
    assert_eq!(editor.scene().elements.len(), 4 + 21);
    assert_eq!(editor.selection().count(), 20, "21 minus the label");
}
