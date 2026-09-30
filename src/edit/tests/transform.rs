//! Resize, rotate and endpoint handles.
use super::{SHIFT, drag, editor};
use crate::edit::{Command, Editor, Handle, Modifiers, Pointer, Tool};
use crate::scene::{Kind, Scene};

const NONE: Modifiers = Modifiers {
    shift: false,
    alt: false,
    command: false,
};

fn handle(editor: &Editor, which: Handle) -> [f64; 2] {
    editor
        .handles()
        .unwrap()
        .handles
        .iter()
        .find(|(h, _)| *h == which)
        .unwrap()
        .1
}

fn by(p: [f64; 2], d: [f64; 2]) -> [f64; 2] {
    [p[0] + d[0], p[1] + d[1]]
}

fn select_a(editor: &mut Editor) {
    drag(editor, [50.0, 25.0], [50.0, 25.0], NONE);
}

fn box_of(editor: &Editor, index: usize) -> (f64, f64, f64, f64) {
    let b = &editor.scene().elements[index].base;
    (b.x, b.y, b.width, b.height)
}

#[test]
fn handles_sit_where_excalidraw_draws_them() {
    let mut editor = editor();
    select_a(&mut editor);
    // Box 0..100 x 0..50 at zoom 1: nw spans [-8, 0], knob centre 20 above.
    assert_eq!(handle(&editor, Handle::Nw), [-4.0, -4.0]);
    assert_eq!(handle(&editor, Handle::Se), [104.0, 54.0]);
    assert_eq!(handle(&editor, Handle::Rotation), [50.0, -20.0]);
    assert_eq!(
        editor.handle_at([104.0, 25.0]),
        Some(Handle::E),
        "right border"
    );
    assert_eq!(editor.handle_at([50.0, 25.0]), None);
}

#[test]
fn corner_drag_resizes_and_shift_keeps_aspect() {
    let mut editor = editor();
    select_a(&mut editor);
    let se = handle(&editor, Handle::Se);
    drag(&mut editor, se, by(se, [50.0, 50.0]), NONE);
    assert_eq!(box_of(&editor, 0), (0.0, 0.0, 150.0, 100.0));
    editor.command(Command::Undo);
    assert_eq!(box_of(&editor, 0), (0.0, 0.0, 100.0, 50.0));

    let se = handle(&editor, Handle::Se);
    drag(&mut editor, se, by(se, [100.0, 10.0]), SHIFT);
    assert_eq!(box_of(&editor, 0), (0.0, 0.0, 200.0, 100.0));
}

#[test]
fn alt_resizes_from_the_centre() {
    let mut editor = editor();
    select_a(&mut editor);
    let alt = Modifiers { alt: true, ..NONE };
    drag(&mut editor, [104.0, 25.0], [124.0, 25.0], alt);
    assert_eq!(box_of(&editor, 0), (-20.0, 0.0, 140.0, 50.0));
}

#[test]
fn rotation_knob_sets_the_angle_and_shift_snaps() {
    let mut editor = editor();
    select_a(&mut editor);
    let knob = handle(&editor, Handle::Rotation);
    drag(&mut editor, knob, [100.0, 25.0], NONE);
    let angle = editor.scene().elements[0].base.angle;
    assert!(
        (angle - std::f64::consts::FRAC_PI_2).abs() < 1e-9,
        "{angle}"
    );
    editor.command(Command::Undo);
    let knob = handle(&editor, Handle::Rotation);
    drag(&mut editor, knob, [100.0, 22.0], SHIFT);
    let angle = editor.scene().elements[0].base.angle;
    assert!(
        (angle - std::f64::consts::FRAC_PI_2).abs() < 1e-9,
        "snapped: {angle}"
    );
}

#[test]
fn resizing_a_container_recentres_its_label() {
    let mut editor = editor();
    drag(&mut editor, [250.0, 25.0], [250.0, 25.0], NONE);
    let se = handle(&editor, Handle::Se);
    drag(&mut editor, se, by(se, [100.0, 50.0]), NONE);
    let label = &editor.scene().elements[2].base;
    // Box 200..400 x 0..100; "hi" re-measures to 22 x 25.
    assert_eq!((label.x, label.y), (289.0, 37.5));
}

#[test]
fn resizing_a_labelled_shape_rewraps_and_grows_it_away_from_the_handle() {
    let mut editor = editor();
    editor.command(Command::Tool(Tool::Text));
    drag(&mut editor, [50.0, 25.0], [50.0, 25.0], NONE);
    editor.set_text("hello world foo");
    editor.finish_text();
    // Wrapped to 3 lines; a is 100 x 85.
    editor.command(Command::Tool(Tool::Selection));
    select_a(&mut editor);
    assert!(editor.is_selected("a"));
    // The right border, like `handles_sit_where_excalidraw_draws_them`.
    assert_eq!(editor.handle_at([104.0, 42.5]), Some(Handle::E));
    drag(&mut editor, [104.0, 42.5], [304.0, 42.5], NONE);
    let label = editor.scene().elements.last().unwrap();
    let Kind::Text(text) = &label.kind else {
        panic!("label")
    };
    assert_eq!(text.text, "hello world foo", "one line in 300 - 10");
    assert_eq!(box_of(&editor, 0), (0.0, 0.0, 300.0, 85.0), "never shrinks");

    // Narrow it from the top-left corner: 40 fits 3 characters, so long
    // words break; 5 lines need 135, and a grows up.
    let nw = handle(&editor, Handle::Nw);
    drag(&mut editor, nw, by(nw, [250.0, 0.0]), NONE);
    let Kind::Text(text) = &editor.scene().elements.last().unwrap().kind else {
        panic!("label")
    };
    assert_eq!(text.text, "hel\nlo\nwor\nld\nfoo");
    let (x, y, w, h) = box_of(&editor, 0);
    assert_eq!((x, w), (250.0, 50.0));
    assert_eq!((y + h, h), (85.0, 135.0), "bottom edge kept");
}

#[test]
fn multi_point_lines_scale_their_points() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Line));
    for at in [[0.0, 100.0], [50.0, 150.0], [100.0, 100.0], [100.0, 100.0]] {
        editor.pointer(Pointer::Hover, at, NONE);
        editor.pointer(Pointer::Down, at, NONE);
        editor.pointer(Pointer::Up, at, NONE);
    }
    let se = handle(&editor, Handle::Se);
    drag(&mut editor, se, by(se, [100.0, 50.0]), NONE);
    let Kind::Line(line) = &editor.scene().elements[0].kind else {
        panic!("line")
    };
    assert_eq!(line.points, [[0.0, 0.0], [100.0, 100.0], [200.0, 0.0]]);
    assert_eq!(box_of(&editor, 0), (0.0, 100.0, 200.0, 100.0));
}

#[test]
fn two_point_arrows_drag_their_endpoints() {
    let mut editor = editor();
    drag(&mut editor, [150.0, 0.0], [150.0, 0.0], NONE);
    let handles = editor.handles().unwrap();
    assert_eq!(handles.points, [[100.0, 0.0], [200.0, 0.0]]);
    assert!(handles.handles.is_empty());
    drag(&mut editor, [200.0, 0.0], [200.0, 50.0], NONE);
    drag(&mut editor, [100.0, 0.0], [120.0, 10.0], NONE);
    let arrow = &editor.scene().elements[3];
    let Kind::Arrow(line) = &arrow.kind else {
        panic!("arrow")
    };
    assert_eq!((arrow.base.x, arrow.base.y), (120.0, 10.0));
    assert_eq!(line.points, [[0.0, 0.0], [80.0, 40.0]]);
}

#[test]
fn labelled_shapes_keep_one_character_and_scale_their_label_with_shift() {
    let labelled = || {
        let mut editor = editor();
        editor.command(Command::Tool(Tool::Text));
        drag(&mut editor, [50.0, 25.0], [50.0, 25.0], NONE);
        editor.set_text("hi");
        editor.finish_text();
        editor.command(Command::Tool(Tool::Selection));
        select_a(&mut editor);
        editor
    };
    let font = |editor: &Editor| match &editor.scene().elements.last().unwrap().kind {
        Kind::Text(text) => text.font_size,
        _ => panic!("label"),
    };

    // Pulling the right edge past the left one stops at one character
    // (ApproxMeasure: 11 wide at 20) plus padding, without flipping.
    let mut editor = labelled();
    assert_eq!(editor.handle_at([104.0, 25.0]), Some(Handle::E));
    drag(&mut editor, [104.0, 25.0], [-96.0, 25.0], NONE);
    let (x, _, w, _) = box_of(&editor, 0);
    assert_eq!((x, w), (0.0, 21.0));
    assert_eq!(font(&editor), 20.0);

    // Shift keeps the aspect ratio and scales the font with the label's
    // room: 90 wide before, 190 after.
    let mut editor = labelled();
    let se = handle(&editor, Handle::Se);
    drag(&mut editor, se, by(se, [100.0, 50.0]), SHIFT);
    assert_eq!(box_of(&editor, 0), (0.0, 0.0, 200.0, 100.0));
    assert_eq!(font(&editor), 20.0 * 190.0 / 90.0);
}

#[test]
fn releasing_shift_mid_resize_restores_the_label_font() {
    let mut editor = editor();
    editor.command(Command::Tool(Tool::Text));
    drag(&mut editor, [50.0, 25.0], [50.0, 25.0], NONE);
    editor.set_text("hi");
    editor.finish_text();
    editor.command(Command::Tool(Tool::Selection));
    select_a(&mut editor);
    let se = handle(&editor, Handle::Se);
    editor.pointer(Pointer::Down, se, NONE);
    editor.pointer(Pointer::Move, by(se, [100.0, 50.0]), SHIFT);
    editor.pointer(Pointer::Move, by(se, [100.0, 60.0]), NONE);
    editor.pointer(Pointer::Up, by(se, [100.0, 60.0]), NONE);
    let Kind::Text(text) = &editor.scene().elements.last().unwrap().kind else {
        panic!("label")
    };
    assert_eq!(text.font_size, 20.0);
}

#[test]
fn shift_resizing_a_labelled_arrow_scales_the_font_with_its_width() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Arrow));
    for at in [[0.0, 0.0], [100.0, 100.0], [200.0, 0.0], [200.0, 0.0]] {
        editor.pointer(Pointer::Hover, at, NONE);
        editor.pointer(Pointer::Down, at, NONE);
        editor.pointer(Pointer::Up, at, NONE);
    }
    editor.double_click([100.0, 100.0]);
    editor.set_text("hi");
    editor.finish_text();
    editor.command(Command::Escape);
    let arrow = editor.scene().elements[0].base.id.clone();
    drag(&mut editor, [50.0, 50.0], [50.0, 50.0], NONE);
    assert!(editor.is_selected(&arrow));
    let se = handle(&editor, Handle::Se);
    let width = editor.scene().elements[0].base.width;
    drag(&mut editor, se, by(se, [width, 0.0]), SHIFT);
    let Kind::Text(text) = &editor.scene().elements.last().unwrap().kind else {
        panic!("label")
    };
    let grown = editor.scene().elements[0].base.width / width;
    assert!(
        (text.font_size - 20.0 * grown).abs() < 1e-9,
        "{}",
        text.font_size
    );
    assert!(grown > 1.5);
}

#[test]
fn uniform_multi_resize_scales_label_fonts() {
    let mut editor = editor();
    drag(&mut editor, [50.0, 25.0], [50.0, 25.0], NONE);
    drag(&mut editor, [250.0, 25.0], [250.0, 25.0], SHIFT);
    let se = handle(&editor, Handle::Se);
    // Common box 0..300 x 0..50; Shift doubles it.
    drag(&mut editor, se, by(se, [300.0, 50.0]), SHIFT);
    assert_eq!(box_of(&editor, 1), (400.0, 0.0, 200.0, 100.0));
    let label = &editor.scene().elements[2];
    let Kind::Text(text) = &label.kind else {
        panic!("label")
    };
    assert_eq!(text.font_size, 40.0);
    // "hi" at 40: 44 x 50, centred in b.
    assert_eq!((label.base.x, label.base.y), (478.0, 25.0));
}

#[test]
fn line_points_drag_and_a_midpoint_adds_a_point() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Line));
    drag(&mut editor, [0.0, 0.0], [100.0, 0.0], NONE);
    let handles = editor.handles().unwrap();
    assert!(handles.handles.is_empty(), "no box for a 2-point line");
    assert_eq!(handles.midpoints, [(1, [50.0, 0.0])]);

    drag(&mut editor, [50.0, 0.0], [50.0, 40.0], NONE);
    let points = |editor: &Editor| match &editor.scene().elements[0].kind {
        Kind::Line(line) => line.points.clone(),
        _ => unreachable!(),
    };
    assert_eq!(points(&editor), [[0.0, 0.0], [50.0, 40.0], [100.0, 0.0]]);
    let handles = editor.handles().unwrap();
    assert!(!handles.handles.is_empty(), "3 points get the box too");
    assert_eq!(handles.points.len(), 3);
    assert!(
        handles.midpoints.is_empty(),
        "3+ points: only in the line editor"
    );

    // Any point drags; the first re-bases the line.
    drag(&mut editor, [0.0, 0.0], [-10.0, 5.0], NONE);
    assert_eq!(points(&editor), [[0.0, 0.0], [60.0, 35.0], [110.0, -5.0]]);
    assert_eq!(
        (
            editor.scene().elements[0].base.x,
            editor.scene().elements[0].base.y
        ),
        (-10.0, 5.0)
    );
    editor.command(Command::Undo);
    editor.command(Command::Undo);
    assert_eq!(points(&editor), [[0.0, 0.0], [100.0, 0.0]]);
}

#[test]
fn line_editor_selects_drags_adds_and_deletes_points() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Line));
    for at in [[0.0, 0.0], [100.0, 0.0], [200.0, 0.0], [200.0, 0.0]] {
        editor.pointer(Pointer::Hover, at, NONE);
        editor.pointer(Pointer::Down, at, NONE);
        editor.pointer(Pointer::Up, at, NONE);
    }
    let points = |editor: &Editor| match &editor.scene().elements[0].kind {
        Kind::Line(line) => line.points.clone(),
        _ => unreachable!(),
    };
    assert_eq!(points(&editor).len(), 3);
    editor.command(Command::EditLine);
    let id = editor.scene().elements[0].base.id.clone();
    assert_eq!(editor.editing_line(), Some(id.as_str()));
    let handles = editor.handles().unwrap();
    assert!(handles.editing_line && handles.handles.is_empty());
    // Round lines have their middles on the rough curve, so near the
    // chords' middles, not on them (the seed is random).
    let ends: Vec<usize> = handles.midpoints.iter().map(|m| m.0).collect();
    assert_eq!(ends, [1, 2]);
    for (&(_, [x, y]), cx) in handles.midpoints.iter().zip([50.0, 150.0]) {
        assert!((x - cx).abs() < 5.0 && y.abs() < 5.0, "middle at {x}, {y}");
    }

    // Select the middle and the last point, then drag both.
    drag(&mut editor, [100.0, 0.0], [100.0, 0.0], NONE);
    drag(&mut editor, [200.0, 0.0], [200.0, 0.0], SHIFT);
    assert_eq!(editor.handles().unwrap().selected_points, [1, 2]);
    drag(&mut editor, [200.0, 0.0], [200.0, 30.0], NONE);
    assert_eq!(points(&editor), [[0.0, 0.0], [100.0, 30.0], [200.0, 30.0]]);

    // A segment middle adds a point; Delete removes the selected one.
    let middle = editor.handles().unwrap().midpoints[0].1;
    drag(&mut editor, middle, [50.0, -20.0], NONE);
    assert_eq!(points(&editor).len(), 4);
    assert_eq!(editor.handles().unwrap().selected_points, [1]);
    editor.command(Command::Delete);
    assert_eq!(points(&editor), [[0.0, 0.0], [100.0, 30.0], [200.0, 30.0]]);
    assert!(!editor.scene().elements[0].base.is_deleted);

    // Escape leaves the editor, keeping the line selected.
    editor.command(Command::Escape);
    assert_eq!(editor.editing_line(), None);
    assert!(editor.is_selected(&id));
    editor.command(Command::Undo);
    assert_eq!(points(&editor).len(), 4, "the delete was one undo step");
}

#[test]
fn freedraw_strokes_select_on_their_path_and_resize_their_points() {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/scenes/freedraw.excalidraw"
    ))
    .unwrap();
    let mut editor = Editor::new(serde_json::from_str::<Scene>(&json).unwrap());
    let selected = |editor: &Editor| {
        editor
            .selection()
            .map(|e| e.base.id.clone())
            .collect::<Vec<_>>()
    };
    // On the scribble's wave (point 6 is at 18, 19.95).
    drag(&mut editor, [18.0, 20.0], [18.0, 20.0], NONE);
    assert_eq!(selected(&editor), ["scribble"]);
    // Inside the filled loop, far from its outline.
    drag(&mut editor, [170.0, 110.0], [170.0, 110.0], NONE);
    assert_eq!(selected(&editor), ["loop"]);
    // Beside the 2-point stroke from (380, 10) to (440, 40): nothing.
    drag(&mut editor, [380.0, 40.0], [380.0, 40.0], NONE);
    assert!(selected(&editor).is_empty());

    drag(&mut editor, [410.0, 25.0], [410.0, 25.0], NONE);
    assert_eq!(selected(&editor), ["two"]);
    let se = handle(&editor, Handle::Se);
    drag(&mut editor, se, by(se, [60.0, 30.0]), NONE);
    let two = editor
        .scene()
        .elements
        .iter()
        .find(|e| e.base.id == "two")
        .unwrap();
    assert_eq!(two.freedraw().unwrap().points, [[0.0, 0.0], [120.0, 60.0]]);
    assert_eq!(
        (two.base.x, two.base.y, two.base.width, two.base.height),
        (380.0, 10.0, 120.0, 60.0)
    );
    let saved = serde_json::to_string(editor.scene()).unwrap();
    assert!(
        saved.contains("[120,60]"),
        "points keep JavaScript's number spelling"
    );
}

#[test]
fn a_selected_arrows_points_and_middle_are_in_reach_within_11_px() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Arrow));
    drag(&mut editor, [0.0, 0.0], [200.0, 0.0], NONE);
    // Excalidraw's reach: 11 screen px around a point or segment middle.
    assert_eq!(editor.hovered_point([190.0, 3.0]), Some([200.0, 0.0]));
    assert!(editor.over_line_handle([190.0, 3.0]));
    assert!(editor.over_line_handle([100.0, 10.0]), "the middle");
    assert_eq!(
        editor.hovered_point([100.0, 0.0]),
        None,
        "no halo on a middle"
    );
    assert!(!editor.over_line_handle([50.0, 0.0]), "the body");
    // Zoomed in 2x, 11 px is 5.5 units.
    editor.set_zoom(2.0);
    assert_eq!(editor.hovered_point([194.0, 0.0]), None);
    assert!(editor.hovered_point([195.0, 0.0]).is_some());
    // A press there drags the point.
    drag(&mut editor, [195.0, 0.0], [195.0, 40.0], NONE);
    let Kind::Arrow(line) = &editor.scene().elements[0].kind else {
        panic!("arrow")
    };
    assert_eq!(line.points[1], [200.0, 40.0]);
}
