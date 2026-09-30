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

#[test]
fn the_frame_tool_takes_in_what_lies_wholly_inside() {
    let mut editor = editor();
    editor.command(Command::Tool(crate::edit::Tool::Frame));
    // Around the free diamond at (120, 240), 120 x 80.
    drag(
        &mut editor,
        [100.0, 220.0],
        [260.0, 340.0],
        Modifiers::default(),
    );
    let frame = editor.selection().next().unwrap();
    assert_eq!(frame.frame_title(), Some("Frame"));
    let id = frame.base.id.clone();
    let json = serde_json::to_value(editor.scene().saved()).unwrap();
    let saved = json["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id.as_str())
        .unwrap();
    assert_eq!(
        (&saved["type"], &saved["name"], &saved["strokeColor"]),
        (&"frame".into(), &serde_json::Value::Null, &"#bbb".into())
    );
    assert_eq!(get(&editor, "free").frame_id(), Some(id.as_str()));

    // Another frame's child stays where it is.
    editor.command(Command::Tool(crate::edit::Tool::Frame));
    drag(
        &mut editor,
        [410.0, 80.0],
        [560.0, 220.0],
        Modifiers::default(),
    );
    assert_eq!(get(&editor, "poke").frame_id(), Some("two"));

    editor.command(Command::Undo);
    editor.command(Command::Undo);
    assert_eq!(get(&editor, "free").frame_id(), None, "one undo step each");
}

#[test]
fn dragging_into_or_out_of_a_frame_changes_membership() {
    let mut editor = editor();
    let move_by = |editor: &mut Editor, from: [f64; 2], by: [f64; 2]| {
        let to = [from[0] + by[0], from[1] + by[1]];
        drag(editor, from, from, Modifiers::default());
        drag(editor, from, to, Modifiers::default());
    };
    // The free diamond, grabbed on its outline, dropped inside frame "one".
    move_by(&mut editor, [150.0, 260.0], [-60.0, -150.0]);
    assert_eq!(get(&editor, "free").frame_id(), Some("one"));
    // Dragged within the frame, "inside" stays; dragged far off, it leaves.
    // (Grabbed right of the diamond, which now sits over its left part.)
    move_by(&mut editor, [250.0, 100.0], [0.0, 10.0]);
    assert_eq!(get(&editor, "inside").base.y, 70.0);
    assert_eq!(get(&editor, "inside").frame_id(), Some("one"));
    move_by(&mut editor, [250.0, 110.0], [0.0, 400.0]);
    assert_eq!(get(&editor, "inside").frame_id(), None);
    editor.command(Command::Undo);
    assert_eq!(
        get(&editor, "inside").frame_id(),
        Some("one"),
        "one undo step with the move"
    );
}
