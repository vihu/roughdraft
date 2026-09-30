//! Bugs found by reviews of the slices, one test each.
use super::{drag, editor};
use crate::edit::{Command, Editor, Handle, Modifiers, Pointer, StyleChange, Tool};
use crate::scene::{Kind, Scene, TextAlign};

const NONE: Modifiers = Modifiers {
    shift: false,
    alt: false,
    command: false,
};

fn live(editor: &Editor) -> usize {
    editor
        .scene()
        .elements
        .iter()
        .filter(|e| !e.base.is_deleted)
        .count()
}

#[test]
fn a_command_during_a_drag_abandons_the_drag() {
    // Undo used to shrink the scene under a drawing gesture, whose stored
    // index then panicked in `active`.
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Rectangle));
    drag(&mut editor, [0.0, 0.0], [50.0, 50.0], NONE);
    editor.command(Command::Tool(Tool::Rectangle));
    editor.pointer(Pointer::Down, [100.0, 100.0], NONE);
    editor.pointer(Pointer::Move, [150.0, 150.0], NONE);
    editor.command(Command::Undo);
    editor.pointer(Pointer::Move, [160.0, 160.0], NONE);
    assert!(editor.active().is_empty());
    editor.pointer(Pointer::Up, [160.0, 160.0], NONE);
    assert_eq!(
        live(&editor),
        0,
        "the drag is dropped, the undo removes the first"
    );
}

#[test]
fn cut_and_paste_while_typing_commit_the_text_first() {
    let mut editor = editor();
    drag(&mut editor, [50.0, 25.0], [50.0, 25.0], NONE);
    editor.double_click([500.0, 500.0]);
    editor.set_text("hello");
    let json = editor.cut().unwrap();
    assert!(
        json.contains("hello") && !json.contains(r#""id":"a""#),
        "{json}"
    );
    assert_eq!(live(&editor), 4, "`a` stays");

    editor.double_click([500.0, 500.0]);
    editor.set_text("typed");
    let count = live(&editor);
    assert!(editor.paste("pasted", [600.0, 600.0]));
    editor.finish_text();
    assert_eq!(live(&editor), count + 1);
    editor.command(Command::Undo);
    let texts: Vec<String> = editor
        .scene()
        .elements
        .iter()
        .filter(|e| !e.base.is_deleted)
        .filter_map(|e| match &e.kind {
            Kind::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"typed".to_owned()), "{texts:?}");
    assert!(!texts.contains(&"pasted".to_owned()));
}

#[test]
fn an_image_pulled_past_its_opposite_corner_mirrors() {
    let mut editor = Editor::new(Scene::default());
    editor.insert_image(
        "data:image/png;base64,TWFu".into(),
        "image/png",
        [100.0, 50.0],
        [50.0, 25.0],
    );
    let se = editor
        .handles()
        .unwrap()
        .handles
        .iter()
        .find(|(h, _)| *h == Handle::Se)
        .unwrap()
        .1;
    drag(&mut editor, se, [-150.0, -100.0], NONE);
    let image = &editor.scene().elements[0];
    assert_eq!(image.image_flip(), [true, true]);
}

#[test]
fn pasting_a_shape_style_onto_text_gives_it_the_default_font() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Text));
    drag(&mut editor, [0.0, 0.0], [0.0, 0.0], NONE);
    editor.set_text("x");
    editor.finish_text();
    editor.apply_style(StyleChange::FontSize(36.0));
    editor.apply_style(StyleChange::TextAlign(TextAlign::Right));
    editor.command(Command::Tool(Tool::Rectangle));
    drag(&mut editor, [100.0, 100.0], [200.0, 200.0], NONE);
    editor.command(Command::CopyStyles);
    drag(&mut editor, [5.0, 10.0], [5.0, 10.0], NONE);
    editor.command(Command::PasteStyles);
    let Kind::Text(text) = &editor.scene().elements[0].kind else {
        panic!("text")
    };
    assert_eq!(
        (text.font_size, text.font_family, &text.text_align),
        (20.0, 5, &TextAlign::Left)
    );
}

#[test]
fn a_labelled_bound_arrow_flips_instead_of_swapping_heads() {
    // Excalidraw's selection counts the label, so it is not "only bound
    // arrows" there.
    let mut editor = editor();
    editor.double_click([150.0, 0.0]);
    editor.set_text("go");
    editor.finish_text();
    let heads = |editor: &Editor| match &editor.scene().elements[3].kind {
        Kind::Arrow(line) => (line.start_arrowhead.clone(), line.points.clone()),
        _ => panic!("arrow"),
    };
    let (start, points) = heads(&editor);
    assert!(editor.is_selected("r"));
    editor.command(Command::Flip(crate::edit::Axis::Horizontal));
    let (flipped_start, flipped_points) = heads(&editor);
    assert_eq!(flipped_start, start, "heads stay");
    assert_ne!(flipped_points, points, "the points mirror");
}

#[test]
fn pasting_while_the_line_editor_is_open_closes_it() {
    // It kept point indices of the old line and panicked on the new one.
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Line));
    drag(&mut editor, [0.0, 0.0], [100.0, 0.0], NONE);
    let json = editor.copy().unwrap();
    editor.command(Command::Tool(Tool::Line));
    for at in [
        [0.0, 100.0],
        [100.0, 100.0],
        [200.0, 100.0],
        [300.0, 100.0],
        [300.0, 100.0],
    ] {
        editor.pointer(Pointer::Hover, at, NONE);
        editor.pointer(Pointer::Down, at, NONE);
        editor.pointer(Pointer::Up, at, NONE);
    }
    editor.command(Command::EditLine);
    drag(&mut editor, [300.0, 100.0], [300.0, 100.0], NONE);
    assert!(editor.paste(&json, [150.0, 300.0]));
    assert_eq!(editor.editing_line(), None);
    let shift = Modifiers {
        shift: true,
        ..NONE
    };
    drag(&mut editor, [100.0, 300.0], [100.0, 300.0], shift);
}

#[test]
fn shift_drawing_back_to_the_start_keeps_points_finite() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Line));
    let shift = Modifiers {
        shift: true,
        ..NONE
    };
    editor.pointer(Pointer::Down, [0.0, 0.0], shift);
    editor.pointer(Pointer::Move, [50.0, 0.0], shift);
    editor.pointer(Pointer::Move, [0.0, 0.0], shift);
    editor.pointer(Pointer::Up, [0.0, 0.0], shift);
    for element in &editor.scene().elements {
        if let Kind::Line(line) = &element.kind {
            assert!(line.points.iter().flatten().all(|v| v.is_finite()));
        }
    }
}

#[test]
fn text_left_empty_leaves_no_undo_step() {
    let mut editor = editor();
    drag(&mut editor, [50.0, 25.0], [50.0, 25.0], NONE);
    editor.command(Command::Nudge([5.0, 0.0]));
    editor.command(Command::Undo);
    editor.command(Command::Tool(Tool::Text));
    drag(&mut editor, [500.0, 500.0], [500.0, 500.0], NONE);
    editor.command(Command::Escape);
    editor.command(Command::Redo);
    assert_eq!(super::x(&editor, "a"), 5.0, "the redo survives");
}

#[test]
fn text_resize_stops_at_size_one_and_shift_rotation_stores_zero() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Text));
    drag(&mut editor, [0.0, 0.0], [0.0, 0.0], NONE);
    editor.set_text("hi");
    editor.finish_text();
    let se = |editor: &Editor| {
        editor
            .handles()
            .unwrap()
            .handles
            .iter()
            .find(|(h, _)| *h == Handle::Se)
            .unwrap()
            .1
    };
    let corner = se(&editor);
    drag(&mut editor, corner, [0.5, 0.5], NONE);
    let corner = se(&editor);
    drag(&mut editor, corner, [-100.0, -100.0], NONE);
    let text = &editor.scene().elements[0];
    let Kind::Text(t) = &text.kind else {
        panic!("text")
    };
    assert!(t.font_size >= 1.0, "{}", t.font_size);

    // Just left of straight up with Shift snaps to 0, not 2π.
    let knob = editor
        .handles()
        .unwrap()
        .handles
        .iter()
        .find(|(h, _)| *h == Handle::Rotation)
        .unwrap()
        .1;
    let b = &editor.scene().elements[0].base;
    let centre = [b.x + b.width / 2.0, b.y + b.height / 2.0];
    let shift = Modifiers {
        shift: true,
        ..NONE
    };
    drag(
        &mut editor,
        knob,
        [centre[0] - 0.5, centre[1] - 200.0],
        shift,
    );
    assert_eq!(editor.scene().elements[0].base.angle, 0.0);
}

#[test]
fn enter_on_a_line_does_not_start_text() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Line));
    drag(&mut editor, [0.0, 0.0], [100.0, 0.0], NONE);
    editor.command(Command::Finish);
    assert!(editor.editing().is_none());
    assert_eq!(editor.scene().elements.len(), 1);
}

fn end_binding(editor: &Editor, index: usize) -> Option<String> {
    editor.scene().elements[index]
        .binding(crate::scene::ArrowEnd::End)
        .map(|b| b.element_id)
}

#[test]
fn moved_arrows_keep_only_bindings_in_reach_and_pick_up_none() {
    // `r` (index 3) runs from `a` to `b` and is bound to `b` at its end.
    let mut editor = editor();
    drag(&mut editor, [150.0, 0.0], [150.0, 0.0], NONE);
    assert!(editor.is_selected("r"));
    editor.command(Command::Nudge([0.0, 5.0]));
    assert_eq!(
        end_binding(&editor, 3).as_deref(),
        Some("b"),
        "still in reach"
    );
    for _ in 0..60 {
        editor.command(Command::Nudge([0.0, 5.0]));
    }
    assert_eq!(end_binding(&editor, 3), None, "300 away lets go");
    let b = editor
        .scene()
        .elements
        .iter()
        .find(|e| e.base.id == "b")
        .unwrap();
    assert!(b.base.bound_elements.iter().flatten().all(|r| r.id != "r"));

    // Dragged back onto `b` as a whole, it does not bind again.
    drag(&mut editor, [150.0, 305.0], [150.0, 0.0], NONE);
    assert_eq!(end_binding(&editor, 3), None);
}

#[test]
fn arrows_bind_inside_solid_filled_shapes_but_never_to_locked_ones() {
    // `a` (0..100 x 0..50) has a solid blue fill.
    let mut editor = editor();
    editor.command(Command::Tool(Tool::Arrow));
    drag(&mut editor, [50.0, 300.0], [50.0, 25.0], NONE);
    let arrow = editor.scene().elements.len() - 1;
    assert_eq!(end_binding(&editor, arrow).as_deref(), Some("a"));

    let mut editor = super::editor();
    drag(&mut editor, [50.0, 25.0], [50.0, 25.0], NONE);
    editor.command(Command::ToggleElementLock);
    editor.command(Command::Tool(Tool::Arrow));
    drag(&mut editor, [50.0, 300.0], [50.0, 2.0], NONE);
    let arrow = editor.scene().elements.len() - 1;
    assert_eq!(end_binding(&editor, arrow), None, "a is locked");
}

/// `x` at 0, then `g1` at 200 and `g2` at 400 in group `G`; all filled,
/// 100 x 50.
fn grouped() -> Editor {
    let shape = |id: &str, x: u32, groups: &str| {
        format!(
            r##"{{"id":"{id}","type":"rectangle","x":{x},"y":0,"width":100,"height":50,"angle":0,"strokeColor":"#1e1e1e","backgroundColor":"#a5d8ff","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roundness":null,"roughness":1,"opacity":100,"seed":1,"version":1,"versionNonce":1,"isDeleted":false,"boundElements":null,"groupIds":[{groups}]}}"##
        )
    };
    let json = format!(
        r#"{{"type":"excalidraw","elements":[{},{},{}]}}"#,
        shape("x", 0, ""),
        shape("g1", 200, r#""G""#),
        shape("g2", 400, r#""G""#)
    );
    Editor::new(serde_json::from_str::<Scene>(&json).unwrap())
}

fn order(editor: &Editor) -> Vec<String> {
    editor
        .scene()
        .elements
        .iter()
        .map(|e| e.base.id.clone())
        .collect()
}

#[test]
fn groups_move_duplicate_and_select_as_excalidraw_does() {
    // Bring forward passes the whole group.
    let mut editor = grouped();
    drag(&mut editor, [50.0, 25.0], [50.0, 25.0], NONE);
    editor.command(Command::Reorder(crate::edit::Order::Forward));
    assert_eq!(order(&editor), ["g1", "g2", "x"]);

    // A duplicated group's copies stay together after it.
    let mut editor = grouped();
    drag(&mut editor, [250.0, 25.0], [250.0, 25.0], NONE);
    assert!(editor.is_selected("g1") && editor.is_selected("g2"));
    editor.command(Command::Duplicate);
    let ids = order(&editor);
    assert_eq!(&ids[..3], ["x", "g1", "g2"]);
    assert!(
        ids[3..]
            .iter()
            .all(|id| !["x", "g1", "g2"].contains(&id.as_str()))
    );

    // A copy made inside an entered group stays in it.
    let mut editor = grouped();
    editor.double_click([250.0, 25.0]);
    drag(&mut editor, [250.0, 25.0], [250.0, 25.0], NONE);
    assert!(editor.is_selected("g1") && !editor.is_selected("g2"));
    editor.command(Command::Duplicate);
    let copy = editor.selection().next().unwrap();
    assert_eq!(copy.group_ids(), ["G"]);

    // Select all leaves the entered group, so a new group takes everything.
    let mut editor = grouped();
    editor.double_click([250.0, 25.0]);
    editor.command(Command::SelectAll);
    editor.command(Command::Group);
    drag(&mut editor, [600.0, 300.0], [600.0, 300.0], NONE);
    drag(&mut editor, [250.0, 25.0], [250.0, 25.0], NONE);
    assert!(editor.is_selected("x"), "one group of all three");
}

#[test]
fn the_text_tool_edits_free_text_it_clicks() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Text));
    drag(&mut editor, [0.0, 0.0], [0.0, 0.0], NONE);
    editor.set_text("hi");
    editor.finish_text();
    editor.command(Command::Tool(Tool::Text));
    drag(&mut editor, [5.0, 10.0], [5.0, 10.0], NONE);
    let editing = editor.editing().map(|e| e.base.id.clone());
    assert_eq!(
        editing.as_deref(),
        Some(editor.scene().elements[0].base.id.as_str())
    );
    assert_eq!(editor.scene().elements.len(), 1);
}

#[test]
fn the_entered_group_ends_with_a_tool_change_and_bounds_z_order_steps() {
    let alt = Modifiers { alt: true, ..NONE };
    // A tool change leaves the entered group: a new rectangle then steps
    // back past the whole group.
    let mut editor = grouped();
    editor.double_click([250.0, 25.0]);
    editor.command(Command::Tool(Tool::Rectangle));
    drag(&mut editor, [700.0, 0.0], [800.0, 50.0], NONE);
    editor.command(Command::Reorder(crate::edit::Order::Backward));
    let ids = order(&editor);
    let rect = ids
        .iter()
        .position(|id| !["x", "g1", "g2"].contains(&id.as_str()));
    assert_eq!(rect, Some(1), "{ids:?}");

    // In an entered group, a copy on top steps back to just below its
    // sibling, past nothing else.
    let mut editor = grouped();
    editor.double_click([250.0, 25.0]);
    drag(&mut editor, [250.0, 25.0], [250.0, 25.0], NONE);
    drag(&mut editor, [250.0, 25.0], [255.0, 25.0], alt);
    editor.command(Command::Reorder(crate::edit::Order::Backward));
    let ids = order(&editor);
    assert_eq!(
        (ids[0].as_str(), ids[1].as_str(), ids[3].as_str()),
        ("x", "g1", "g2"),
        "{ids:?}"
    );
}

#[test]
fn an_alt_dragged_bound_arrow_stays_bound() {
    let alt = Modifiers { alt: true, ..NONE };
    let mut editor = editor();
    drag(&mut editor, [150.0, 0.0], [150.0, 0.0], NONE);
    drag(&mut editor, [150.0, 0.0], [150.0, 3.0], alt);
    let last = editor.scene().elements.len() - 1;
    assert!(matches!(editor.scene().elements[last].kind, Kind::Arrow(_)));
    assert_eq!(end_binding(&editor, last).as_deref(), Some("b"));
}

#[test]
fn free_text_pulled_past_one_side_keeps_growing() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Text));
    drag(&mut editor, [0.0, 0.0], [0.0, 0.0], NONE);
    editor.set_text("hi");
    editor.finish_text();
    let se = editor
        .handles()
        .unwrap()
        .handles
        .iter()
        .find(|(h, _)| *h == Handle::Se)
        .unwrap()
        .1;
    drag(&mut editor, se, [-5.0, 200.0], NONE);
    let Kind::Text(text) = &editor.scene().elements[0].kind else {
        panic!("text")
    };
    assert!(text.font_size > 20.0, "{}", text.font_size);
}
