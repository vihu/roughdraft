//! Snapping to the grid (REFERENCE-001 section 17).
use super::{drag, editor};
use crate::edit::{Command, Editor, Handle, Modifiers, Pointer, Tool};
use crate::scene::{Grid, Kind, Scene};

const NONE: Modifiers = Modifiers {
    shift: false,
    alt: false,
    command: false,
};

const COMMAND: Modifiers = Modifiers {
    command: true,
    ..NONE
};

fn gridded() -> Editor {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::ToggleGrid);
    editor
}

fn last_box(editor: &Editor) -> (f64, f64, f64, f64) {
    let b = &editor.scene().elements.last().unwrap().base;
    (b.x, b.y, b.width, b.height)
}

fn points(editor: &Editor) -> Vec<[f64; 2]> {
    match &editor.scene().elements.last().unwrap().kind {
        Kind::Line(line) | Kind::Arrow(line) => line.points.clone(),
        other => panic!("expected a line, got {other:?}"),
    }
}

#[test]
fn the_grid_is_saved_in_app_state_and_is_not_an_undo_step() {
    let mut editor = Editor::new(Scene::default());
    assert_eq!(editor.scene().grid(), None);
    editor.command(Command::ToggleGrid);
    assert_eq!(
        editor.scene().grid(),
        Some(Grid {
            size: 20.0,
            step: 5
        })
    );
    let json = serde_json::to_value(editor.scene()).unwrap();
    assert_eq!(json["appState"]["gridModeEnabled"], true);
    editor.command(Command::Undo);
    assert!(editor.scene().grid().is_some());
    editor.command(Command::ToggleGrid);
    let json = serde_json::to_value(editor.scene()).unwrap();
    assert_eq!(json["appState"]["gridModeEnabled"], false);
}

#[test]
fn shapes_start_and_end_on_the_grid_unless_ctrl_is_held() {
    let mut editor = gridded();
    editor.command(Command::Tool(Tool::Rectangle));
    drag(&mut editor, [13.0, 27.0], [107.0, 71.0], NONE);
    assert_eq!(last_box(&editor), (20.0, 20.0, 80.0, 60.0));
    editor.command(Command::Tool(Tool::Rectangle));
    drag(&mut editor, [13.0, 27.0], [107.0, 71.0], COMMAND);
    assert_eq!(last_box(&editor), (13.0, 27.0, 94.0, 44.0));
}

#[test]
fn line_points_snap_as_they_are_drawn() {
    let mut editor = gridded();
    editor.command(Command::Tool(Tool::Arrow));
    drag(&mut editor, [3.0, 4.0], [97.0, 42.0], NONE);
    assert_eq!(points(&editor), [[0.0, 0.0], [100.0, 40.0]]);
    // Click by click, the floating point snaps.
    editor.command(Command::Tool(Tool::Line));
    let click = |editor: &mut Editor, at| {
        editor.pointer(Pointer::Hover, at, NONE);
        editor.pointer(Pointer::Down, at, NONE);
        editor.pointer(Pointer::Up, at, NONE);
    };
    click(&mut editor, [203.0, 4.0]);
    click(&mut editor, [257.0, 38.0]);
    editor.command(Command::Finish);
    assert_eq!(points(&editor), [[0.0, 0.0], [60.0, 40.0]]);
    assert_eq!(last_box(&editor).0, 200.0);
}

#[test]
fn moving_puts_the_selections_top_left_on_the_grid() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Rectangle));
    drag(&mut editor, [5.0, 5.0], [45.0, 35.0], NONE);
    editor.command(Command::ToggleGrid);
    // Moved by (10, 3), its corner (15, 8) goes to the nearest grid point.
    drag(&mut editor, [25.0, 20.0], [35.0, 23.0], NONE);
    assert_eq!(last_box(&editor), (20.0, 0.0, 40.0, 30.0));
}

#[test]
fn resizing_puts_the_dragged_handle_on_the_grid() {
    let mut editor = editor();
    editor.command(Command::ToggleGrid);
    drag(&mut editor, [50.0, 25.0], [50.0, 25.0], NONE);
    // Box 0..100 x 0..50; its se handle sits 4 px out from the corner.
    let se = editor
        .handles()
        .unwrap()
        .handles
        .iter()
        .find(|(h, _)| *h == Handle::Se)
        .unwrap()
        .1;
    drag(&mut editor, se, [se[0] + 13.0, se[1] + 17.0], NONE);
    let a = &editor.scene().elements[0].base;
    assert_eq!((a.x, a.y, a.width, a.height), (0.0, 0.0, 120.0, 60.0));
}

#[test]
fn a_dragged_line_point_lands_on_the_grid() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Arrow));
    drag(&mut editor, [0.0, 0.0], [100.0, 0.0], NONE);
    editor.command(Command::ToggleGrid);
    drag(&mut editor, [100.0, 0.0], [113.0, 27.0], NONE);
    assert_eq!(points(&editor), [[0.0, 0.0], [120.0, 20.0]]);
}

#[test]
fn pasting_puts_the_top_left_on_the_grid() {
    let mut editor = editor();
    drag(&mut editor, [50.0, 25.0], [50.0, 25.0], NONE);
    let copied = editor.copy().unwrap();
    editor.command(Command::ToggleGrid);
    // Centred on (517, 309), the 100 x 50 box's corner would be at
    // (467, 284).
    assert!(editor.paste(&copied, [517.0, 309.0]));
    assert_eq!(last_box(&editor), (460.0, 280.0, 100.0, 50.0));
}

#[test]
fn the_pen_does_not_snap() {
    let mut editor = gridded();
    editor.command(Command::Tool(Tool::Freedraw));
    drag(&mut editor, [13.0, 27.0], [40.0, 30.0], NONE);
    let pen = &editor.scene().elements[0].base;
    assert_eq!((pen.x, pen.y), (13.0, 27.0));
}
