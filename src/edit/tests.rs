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

mod create {
    use super::SHIFT;
    use crate::edit::{Command, Editor, Modifiers, Pointer, Tool};
    use crate::scene::{Arrowhead, Kind, Scene};

    const NONE: Modifiers = Modifiers {
        shift: false,
        alt: false,
        command: false,
    };

    fn empty() -> Editor {
        Editor::new(Scene::default())
    }

    fn drag(editor: &mut Editor, from: [f64; 2], to: [f64; 2], modifiers: Modifiers) {
        editor.pointer(Pointer::Down, from, modifiers);
        editor.pointer(Pointer::Move, to, modifiers);
        editor.pointer(Pointer::Up, to, modifiers);
    }

    fn click(editor: &mut Editor, at: [f64; 2]) {
        editor.pointer(Pointer::Hover, at, NONE);
        editor.pointer(Pointer::Down, at, NONE);
        editor.pointer(Pointer::Up, at, NONE);
    }

    fn points(editor: &Editor) -> Vec<[f64; 2]> {
        match &editor.scene().elements.last().unwrap().kind {
            Kind::Line(line) | Kind::Arrow(line) => line.points.clone(),
            other => panic!("expected a line, got {other:?}"),
        }
    }

    #[test]
    fn drag_creates_a_rectangle_with_excalidraw_defaults() {
        let mut editor = empty();
        editor.command(Command::Tool(Tool::Rectangle));
        drag(&mut editor, [10.0, 10.0], [110.0, 60.0], NONE);
        let json = serde_json::to_value(editor.scene()).unwrap();
        let rect = &json["elements"][0];
        assert_eq!(
            (rect["x"].clone(), rect["y"].clone()),
            (10.into(), 10.into())
        );
        assert_eq!(
            (rect["width"].clone(), rect["height"].clone()),
            (100.into(), 50.into())
        );
        assert_eq!(rect["roundness"], serde_json::json!({"type": 3}));
        assert_eq!(rect["strokeColor"], "#1e1e1e");
        assert_eq!(rect["backgroundColor"], "transparent");
        assert_eq!(rect["fillStyle"], "solid");
        assert_eq!(
            (rect["strokeWidth"].clone(), rect["roughness"].clone()),
            (2.into(), 1.into())
        );
        let id = rect["id"].as_str().unwrap();
        assert!(editor.is_selected(id));
        assert_eq!(editor.tool(), Tool::Selection);
        editor.command(Command::Undo);
        assert!(editor.scene().elements.is_empty());
    }

    #[test]
    fn shift_draws_squares_and_alt_draws_from_the_centre() {
        let mut editor = empty();
        editor.command(Command::Tool(Tool::Ellipse));
        drag(&mut editor, [0.0, 0.0], [40.0, -100.0], SHIFT);
        let e = &editor.scene().elements[0].base;
        assert_eq!((e.x, e.y, e.width, e.height), (0.0, -100.0, 100.0, 100.0));

        editor.command(Command::Tool(Tool::Rectangle));
        let alt = Modifiers { alt: true, ..NONE };
        drag(&mut editor, [50.0, 50.0], [60.0, 70.0], alt);
        let r = &editor.scene().elements[1].base;
        assert_eq!((r.x, r.y, r.width, r.height), (40.0, 30.0, 20.0, 40.0));
    }

    #[test]
    fn a_click_without_drag_draws_nothing_and_keeps_the_tool() {
        let mut editor = empty();
        editor.command(Command::Tool(Tool::Diamond));
        drag(&mut editor, [5.0, 5.0], [5.0, 5.0], NONE);
        assert!(editor.scene().elements.is_empty());
        assert_eq!(editor.tool(), Tool::Diamond);
    }

    #[test]
    fn dragged_arrow_has_two_points_and_shift_snaps_the_angle() {
        let mut editor = empty();
        editor.command(Command::Tool(Tool::Arrow));
        drag(&mut editor, [0.0, 0.0], [100.0, 10.0], SHIFT);
        assert_eq!(points(&editor), [[0.0, 0.0], [100.0, 0.0]]);
        let arrow = &editor.scene().elements[0];
        let Kind::Arrow(line) = &arrow.kind else {
            panic!("arrow")
        };
        assert_eq!(line.end_arrowhead, Some(Arrowhead::Arrow));
        assert_eq!(arrow.base.roundness.as_ref().map(|r| r.kind), Some(2));
        assert_eq!((arrow.base.width, arrow.base.height), (100.0, 0.0));
    }

    #[test]
    fn click_by_click_line_finishes_on_the_last_point() {
        let mut editor = empty();
        editor.command(Command::Tool(Tool::Line));
        click(&mut editor, [10.0, 10.0]);
        assert!(editor.wants_hover());
        click(&mut editor, [110.0, 10.0]);
        click(&mut editor, [110.0, 110.0]);
        click(&mut editor, [112.0, 111.0]);
        assert!(!editor.wants_hover());
        assert_eq!(points(&editor), [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]]);
        let line = &editor.scene().elements[0].base;
        assert_eq!(
            (line.x, line.y, line.width, line.height),
            (10.0, 10.0, 100.0, 100.0)
        );
        assert!(editor.is_selected(&line.id));
        assert_eq!(editor.tool(), Tool::Selection);
    }

    #[test]
    fn clicking_the_start_closes_a_loop() {
        let mut editor = empty();
        editor.command(Command::Tool(Tool::Line));
        for at in [[0.0, 0.0], [100.0, 0.0], [50.0, 80.0], [2.0, 1.0]] {
            click(&mut editor, at);
        }
        assert_eq!(
            points(&editor),
            [[0.0, 0.0], [100.0, 0.0], [50.0, 80.0], [0.0, 0.0]]
        );
        assert!(!editor.wants_hover());
    }

    #[test]
    fn escape_drops_a_line_with_one_point_and_lock_keeps_the_tool() {
        let mut editor = empty();
        editor.command(Command::Tool(Tool::Arrow));
        click(&mut editor, [0.0, 0.0]);
        editor.command(Command::Escape);
        assert!(editor.scene().elements.is_empty());
        assert_eq!(editor.tool(), Tool::Selection);

        editor.command(Command::Tool(Tool::Rectangle));
        editor.command(Command::ToggleLock);
        drag(&mut editor, [0.0, 0.0], [30.0, 30.0], NONE);
        drag(&mut editor, [50.0, 0.0], [80.0, 30.0], NONE);
        assert_eq!(editor.scene().elements.len(), 2);
        assert_eq!(editor.tool(), Tool::Rectangle);
    }
}
