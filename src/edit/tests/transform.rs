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
    // Box 200..400 x 0..100; the label is 100 x 50.
    assert_eq!((label.x, label.y), (250.0, 25.0));
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
