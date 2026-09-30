//! Editor behaviour against REFERENCE-001.
use super::{Command, Editor, Modifiers, Pointer};
use crate::scene::{Kind, Scene};

const BASE: &str = r##""y":0,"width":100,"height":50,"angle":0,"strokeColor":"#1e1e1e","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roundness":null,"roughness":1,"opacity":100,"seed":1,"version":1,"versionNonce":1,"isDeleted":false"##;

/// `a`: filled box at x 0. `b`: outline box at x 200 with label `t`.
/// `r`: an arrow from `a` to `b`, bound to `b`.
fn editor() -> Editor {
    let json = format!(
        r##"{{"type":"excalidraw","elements":[
            {{"id":"a","type":"rectangle","x":0,"backgroundColor":"#a5d8ff","boundElements":null,{BASE}}},
            {{"id":"b","type":"rectangle","x":200,"backgroundColor":"transparent","boundElements":[{{"id":"t","type":"text"}},{{"id":"r","type":"arrow"}}],{BASE}}},
            {{"id":"t","type":"text","x":210,"backgroundColor":"transparent","boundElements":null,{BASE},"text":"hi","fontSize":20,"fontFamily":5,"textAlign":"center","verticalAlign":"middle","lineHeight":1.25,"containerId":"b"}},
            {{"id":"r","type":"arrow","x":100,"backgroundColor":"transparent","boundElements":null,{BASE},"points":[[0,0],[100,0]],"startBinding":null,"endBinding":{{"elementId":"b","focus":0,"gap":1}}}}
        ]}}"##
    );
    Editor::new(serde_json::from_str::<Scene>(&json).unwrap())
}

fn x(editor: &Editor, id: &str) -> f64 {
    editor
        .scene()
        .elements
        .iter()
        .find(|e| e.base.id == id)
        .unwrap()
        .base
        .x
}

fn drag(editor: &mut Editor, from: [f64; 2], to: [f64; 2], modifiers: Modifiers) {
    editor.pointer(Pointer::Down, from, modifiers);
    editor.pointer(Pointer::Move, to, modifiers);
    editor.pointer(Pointer::Up, to, modifiers);
}

const SHIFT: Modifiers = Modifiers {
    shift: true,
    alt: false,
    command: false,
};

#[test]
fn dragging_a_label_moves_its_container_and_undo_restores() {
    let mut editor = editor();
    drag(
        &mut editor,
        [250.0, 25.0],
        [270.0, 25.0],
        Modifiers::default(),
    );
    assert!(editor.is_selected("b"));
    assert_eq!((x(&editor, "b"), x(&editor, "t")), (220.0, 230.0));
    editor.command(Command::Undo);
    assert_eq!((x(&editor, "b"), x(&editor, "t")), (200.0, 210.0));
}

#[test]
fn empty_spot_inside_a_selection_grabs_it_and_shift_locks_axis() {
    let mut editor = editor();
    editor.command(Command::SelectAll);
    // Between `a` and `b`, touching nothing, but inside the selection box.
    editor.pointer(Pointer::Down, [150.0, 40.0], Modifiers::default());
    editor.pointer(Pointer::Move, [170.0, 45.0], SHIFT);
    editor.pointer(Pointer::Up, [170.0, 45.0], SHIFT);
    assert_eq!(
        (x(&editor, "a"), x(&editor, "b"), x(&editor, "r")),
        (20.0, 220.0, 120.0)
    );
    let y = editor.scene().elements[1].base.y;
    assert_eq!(y, 0.0, "shift keeps the dominant axis only");
}

#[test]
fn marquee_selects_contained_and_shift_click_toggles() {
    let mut editor = editor();
    drag(
        &mut editor,
        [-30.0, -30.0],
        [150.0, 60.0],
        Modifiers::default(),
    );
    assert!(editor.is_selected("a") && !editor.is_selected("b"));
    assert_eq!(x(&editor, "a"), 0.0, "a box select, not a drag");
    drag(&mut editor, [250.0, 25.0], [250.0, 25.0], SHIFT);
    assert!(editor.is_selected("a") && editor.is_selected("b"));
    drag(&mut editor, [50.0, 25.0], [50.0, 25.0], SHIFT);
    assert!(!editor.is_selected("a") && editor.is_selected("b"));
}

#[test]
fn click_inside_a_multi_selection_narrows_it() {
    let mut editor = editor();
    editor.command(Command::SelectAll);
    drag(
        &mut editor,
        [50.0, 25.0],
        [50.0, 25.0],
        Modifiers::default(),
    );
    assert!(editor.is_selected("a") && !editor.is_selected("b"));
}

#[test]
fn duplicate_places_copies_after_the_pair_and_remaps_labels() {
    let mut editor = editor();
    drag(
        &mut editor,
        [250.0, 25.0],
        [250.0, 25.0],
        Modifiers::default(),
    );
    editor.command(Command::Duplicate);
    let ids: Vec<&str> = editor
        .scene()
        .elements
        .iter()
        .map(|e| e.base.id.as_str())
        .collect();
    assert_eq!(&ids[..3], ["a", "b", "t"]);
    let (copy, label) = (&editor.scene().elements[3], &editor.scene().elements[4]);
    assert_eq!(copy.base.x, 210.0);
    assert!(editor.is_selected(&copy.base.id) && !editor.is_selected("b"));
    let Kind::Text(text) = &label.kind else {
        panic!("expected the label copy")
    };
    assert_eq!(text.container_id.as_deref(), Some(copy.base.id.as_str()));
    let bound = copy.base.bound_elements.as_ref().unwrap();
    assert_eq!(bound.len(), 1, "arrow binding not copied");
    assert_eq!(bound[0].id, label.base.id);
}

#[test]
fn delete_drops_bindings_and_saves_without_tombstones() {
    let mut editor = editor();
    drag(
        &mut editor,
        [250.0, 25.0],
        [250.0, 25.0],
        Modifiers::default(),
    );
    editor.command(Command::Delete);
    let saved = serde_json::to_value(editor.scene().saved()).unwrap();
    let ids: Vec<&str> = saved["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["a", "r"]);
    assert!(saved["elements"][1]["endBinding"].is_null());
    editor.command(Command::Undo);
    assert!(!editor.scene().elements[1].base.is_deleted);
}

mod binding;
mod clipboard;
mod create;
mod order;
mod style;
mod text;
mod transform;

#[test]
fn alt_drag_leaves_a_copy_behind_and_drags_the_new_one_on_top() {
    let mut editor = editor();
    let alt = Modifiers {
        alt: true,
        ..Modifiers::default()
    };
    drag(&mut editor, [250.0, 25.0], [250.0, 125.0], alt);
    let elements = &editor.scene().elements;
    assert_eq!(elements.len(), 6, "b and its label copied");
    assert_eq!(
        (x(&editor, "b"), elements[1].base.y),
        (200.0, 0.0),
        "original stays"
    );
    let (copy, label) = (&elements[4], &elements[5]);
    assert_eq!((copy.base.x, copy.base.y), (200.0, 100.0));
    let Kind::Text(text) = &label.kind else {
        panic!("label")
    };
    assert_eq!(text.container_id.as_deref(), Some(copy.base.id.as_str()));
    assert_eq!(label.base.y, 100.0);
    assert!(editor.is_selected(&copy.base.id) && !editor.is_selected("b"));
    let arrow = serde_json::to_value(&elements[3]).unwrap();
    assert_eq!(
        arrow["endBinding"]["elementId"], "b",
        "the arrow stays with the original"
    );

    editor.command(Command::Undo);
    assert_eq!(editor.scene().elements.len(), 4);
}

#[test]
fn locked_elements_are_drawn_but_not_selectable() {
    let json = format!(
        r##"{{"type":"excalidraw","elements":[
            {{"id":"under","type":"rectangle","x":0,"backgroundColor":"#a5d8ff","boundElements":null,{BASE}}},
            {{"id":"lock","type":"rectangle","x":0,"backgroundColor":"#a5d8ff","boundElements":null,"locked":true,{BASE}}}
        ]}}"##
    );
    let mut editor = Editor::new(serde_json::from_str::<Scene>(&json).unwrap());
    drag(
        &mut editor,
        [50.0, 25.0],
        [50.0, 25.0],
        Modifiers::default(),
    );
    assert!(editor.is_selected("under"), "the click goes through");
    assert!(!editor.is_selected("lock"));
    editor.command(Command::SelectAll);
    assert!(!editor.is_selected("lock"));
    editor.command(Command::Escape);
    drag(
        &mut editor,
        [-20.0, -20.0],
        [120.0, 70.0],
        Modifiers::default(),
    );
    assert!(
        editor.is_selected("under") && !editor.is_selected("lock"),
        "box select"
    );
}

#[test]
fn element_lock_toggles_and_unlocks_everything_with_nothing_selected() {
    let json = format!(
        r##"{{"type":"excalidraw","elements":[
            {{"id":"under","type":"rectangle","x":0,"backgroundColor":"#a5d8ff","boundElements":null,{BASE}}},
            {{"id":"lock","type":"rectangle","x":0,"backgroundColor":"#a5d8ff","boundElements":null,"locked":true,{BASE}}}
        ]}}"##
    );
    let mut editor = Editor::new(serde_json::from_str::<Scene>(&json).unwrap());
    let click =
        |editor: &mut Editor| drag(editor, [50.0, 25.0], [50.0, 25.0], Modifiers::default());
    click(&mut editor);
    editor.command(Command::ToggleElementLock);
    assert!(editor.scene().elements[0].is_locked());
    assert!(
        editor.handles().is_none(),
        "a locked selection has no handles"
    );
    click(&mut editor);
    assert_eq!(editor.selection().count(), 0, "both are locked now");

    editor.command(Command::ToggleElementLock);
    assert!(editor.scene().elements.iter().all(|e| !e.is_locked()));
    assert!(editor.is_selected("under") && editor.is_selected("lock"));
    editor.command(Command::ToggleElementLock);
    assert!(
        editor.scene().elements.iter().all(|e| e.is_locked()),
        "a selection with nothing locked locks"
    );
    editor.command(Command::Undo);
    assert!(editor.scene().elements.iter().all(|e| !e.is_locked()));
    editor.command(Command::Undo);
    assert!(
        editor.scene().elements.iter().all(|e| e.is_locked()),
        "each toggle is one undo step"
    );
}

#[test]
fn eraser_drag_marks_then_deletes_with_labels_and_alt_unmarks() {
    let mut editor = editor();
    editor.command(Command::Tool(crate::edit::Tool::Eraser));
    // Across a (0..100) and b (200..300) at y 25.
    editor.pointer(Pointer::Down, [50.0, 25.0], Modifiers::default());
    editor.pointer(Pointer::Move, [250.0, 25.0], Modifiers::default());
    let marked = editor.pending_erasure().unwrap().clone();
    assert!(
        marked.contains("a") && marked.contains("b") && marked.contains("t"),
        "{marked:?}"
    );
    assert!(
        editor.active().contains(&"a"),
        "drawn faded on the moving layer"
    );
    // Alt back over b unmarks it and its label.
    let alt = Modifiers {
        alt: true,
        ..Modifiers::default()
    };
    editor.pointer(Pointer::Move, [260.0, 25.0], alt);
    let marked = editor.pending_erasure().unwrap();
    assert!(!marked.contains("b") && !marked.contains("t"));
    editor.pointer(Pointer::Up, [260.0, 25.0], Modifiers::default());

    let deleted = |editor: &Editor, id: &str| {
        editor
            .scene()
            .elements
            .iter()
            .any(|e| e.base.id == id && e.base.is_deleted)
    };
    assert!(deleted(&editor, "a"));
    assert!(
        !deleted(&editor, "r"),
        "the arrow at y 0 is 25 away from the path"
    );
    assert!(!deleted(&editor, "b") && !deleted(&editor, "t"));
    assert_eq!(editor.tool(), crate::edit::Tool::Eraser, "the tool stays");
    editor.command(Command::Undo);
    assert!(!deleted(&editor, "a"));

    // A click erases what is under it; b's label goes with it.
    drag(
        &mut editor,
        [250.0, 25.0],
        [250.0, 25.0],
        Modifiers::default(),
    );
    assert!(deleted(&editor, "b") && deleted(&editor, "t"));
}

#[test]
fn flip_mirrors_the_selection_and_bound_arrows_swap_heads() {
    use crate::edit::Axis;
    let mut editor = editor();
    drag(
        &mut editor,
        [50.0, 25.0],
        [50.0, 25.0],
        Modifiers::default(),
    );
    drag(&mut editor, [250.0, 25.0], [250.0, 25.0], SHIFT);
    // a (0..100) and b (200..300) swap sides about x 150; b's label (as wide
    // as b) follows.
    editor.command(Command::Flip(Axis::Horizontal));
    assert_eq!(
        (x(&editor, "a"), x(&editor, "b"), x(&editor, "t")),
        (200.0, 0.0, 0.0)
    );
    editor.command(Command::Flip(Axis::Horizontal));
    assert_eq!(
        (x(&editor, "a"), x(&editor, "b")),
        (0.0, 200.0),
        "twice is the identity"
    );
    editor.command(Command::Undo);
    assert_eq!(x(&editor, "a"), 200.0);

    // A lone bound arrow keeps its place and swaps its heads.
    let mut editor = crate::edit::tests::editor();
    drag(
        &mut editor,
        [150.0, 0.0],
        [150.0, 0.0],
        Modifiers::default(),
    );
    assert!(editor.is_selected("r"));
    editor.apply_style(crate::edit::StyleChange::EndArrowhead(Some(
        crate::scene::Arrowhead::Arrow,
    )));
    let before = x(&editor, "r");
    editor.command(Command::Flip(Axis::Vertical));
    assert_eq!(x(&editor, "r"), before);
    let json = serde_json::to_value(&editor.scene().elements[3]).unwrap();
    assert_eq!(
        (json["startArrowhead"].clone(), json["endArrowhead"].clone()),
        (serde_json::json!("arrow"), serde_json::Value::Null)
    );

    // Lines mirror their points.
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(crate::edit::Tool::Line));
    for at in [[0.0, 0.0], [100.0, 50.0], [200.0, 0.0], [200.0, 0.0]] {
        editor.pointer(Pointer::Hover, at, Modifiers::default());
        editor.pointer(Pointer::Down, at, Modifiers::default());
        editor.pointer(Pointer::Up, at, Modifiers::default());
    }
    editor.command(Command::Flip(Axis::Vertical));
    let Kind::Line(line) = &editor.scene().elements[0].kind else {
        panic!("line")
    };
    assert_eq!(line.points, [[0.0, 0.0], [100.0, -50.0], [200.0, 0.0]]);
    assert_eq!(
        editor.scene().elements[0].base.y,
        50.0,
        "the box stays in place"
    );
}
