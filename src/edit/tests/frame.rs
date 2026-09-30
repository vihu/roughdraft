//! Frames: selected by their outline or title, carrying their children.
use super::drag;
use crate::edit::{Command, Editor, Handle, Modifiers};
use crate::scene::{Element, Scene};

fn editor() -> Editor {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/scenes/frames.excalidraw"
    ))
    .unwrap();
    Editor::new(serde_json::from_str::<Scene>(&json).unwrap())
}

fn get<'a>(editor: &'a Editor, id: &str) -> &'a Element {
    editor
        .scene()
        .elements
        .iter()
        .find(|e| e.base.id == id)
        .unwrap()
}

fn click(editor: &mut Editor, at: [f64; 2]) {
    drag(editor, at, at, Modifiers::default());
}

#[test]
fn frames_select_by_outline_or_title_and_carry_their_children() {
    let mut editor = editor();
    // Frame "one" is at (0, 0), 300 x 200; its title sits above it.
    click(&mut editor, [20.0, 180.0]);
    assert_eq!(editor.selection().count(), 0, "empty inside of a frame");
    click(&mut editor, [10.0, -10.0]);
    assert!(editor.is_selected("one"), "the title");
    click(&mut editor, [500.0, 500.0]);
    click(&mut editor, [0.5, 100.0]);
    assert!(editor.is_selected("one"), "the outline");
    let handles = editor.handles().unwrap();
    assert!(
        handles.handles.iter().all(|(h, _)| *h != Handle::Rotation),
        "frames do not rotate"
    );

    drag(
        &mut editor,
        [0.5, 100.0],
        [10.5, 110.0],
        Modifiers::default(),
    );
    let at = |editor: &Editor, id: &str| {
        let e = get(editor, id);
        (e.base.x, e.base.y)
    };
    assert_eq!(at(&editor, "one"), (10.0, 10.0));
    assert_eq!(at(&editor, "inside"), (50.0, 70.0), "a child moves along");
    assert_eq!(at(&editor, "note"), (210.0, 30.0));
    assert_eq!(
        at(&editor, "poke"),
        (420.0, 90.0),
        "another frame's child stays"
    );
    editor.command(Command::Nudge([5.0, 0.0]));
    assert_eq!(at(&editor, "inside"), (55.0, 70.0));

    editor.command(Command::Duplicate);
    let copy = editor
        .selection()
        .find(|e| e.frame_title().is_some())
        .unwrap()
        .base
        .id
        .clone();
    let copied_children = editor
        .scene()
        .elements
        .iter()
        .filter(|e| e.frame_id() == Some(copy.as_str()))
        .count();
    assert_eq!(copied_children, 2, "duplicates keep their frame");
}

#[test]
fn deleting_a_frame_keeps_its_children_out_of_it() {
    let mut editor = editor();
    click(&mut editor, [0.5, 100.0]);
    editor.command(Command::Delete);
    assert!(get(&editor, "one").base.is_deleted);
    for id in ["inside", "note"] {
        let child = get(&editor, id);
        assert!(!child.base.is_deleted && child.frame_id().is_none(), "{id}");
        assert!(editor.is_selected(id));
    }
    assert_eq!(get(&editor, "poke").frame_id(), Some("two"));
    editor.command(Command::Undo);
    assert_eq!(get(&editor, "inside").frame_id(), Some("one"));
}

#[test]
fn erasing_a_frame_erases_its_children() {
    let mut editor = editor();
    editor.command(Command::Tool(crate::edit::Tool::Eraser));
    click(&mut editor, [0.5, 100.0]);
    for id in ["one", "inside", "note"] {
        assert!(get(&editor, id).base.is_deleted, "{id}");
    }
    assert!(!get(&editor, "poke").base.is_deleted);
}
