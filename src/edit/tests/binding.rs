//! Arrow binding.
use std::collections::HashSet;

use crate::edit::{Command, Editor, Modifiers, Pointer, Tool};
use crate::scene::{ArrowEnd, Kind, Scene};

const NONE: Modifiers = Modifiers {
    shift: false,
    alt: false,
    command: false,
};

fn drag(editor: &mut Editor, from: [f64; 2], to: [f64; 2]) {
    editor.pointer(Pointer::Down, from, NONE);
    editor.pointer(Pointer::Move, to, NONE);
    editor.pointer(Pointer::Up, to, NONE);
}

/// A 100 x 100 box at the origin and an arrow drawn into its left side,
/// stopping 5 short.
fn bound() -> Editor {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Rectangle));
    drag(&mut editor, [0.0, 0.0], [100.0, 100.0]);
    editor.command(Command::Tool(Tool::Arrow));
    drag(&mut editor, [-100.0, 50.0], [-5.0, 50.0]);
    editor
}

fn end_point(editor: &Editor) -> [f64; 2] {
    let arrow = &editor.scene().elements[1];
    let Kind::Arrow(line) = &arrow.kind else {
        panic!("arrow")
    };
    let last = line.points[line.points.len() - 1];
    [arrow.base.x + last[0], arrow.base.y + last[1]]
}

#[test]
fn drawing_into_a_shape_binds_with_focus_and_gap() {
    let editor = bound();
    let (rect, arrow) = (&editor.scene().elements[0], &editor.scene().elements[1]);
    let binding = arrow.binding(ArrowEnd::End).expect("end bound");
    assert_eq!(binding.element_id, rect.base.id);
    assert!((binding.gap - 5.0).abs() < 1e-9, "gap {}", binding.gap);
    assert!(
        binding.focus.abs() < 1e-9,
        "aimed at the centre: {}",
        binding.focus
    );
    assert!(arrow.binding(ArrowEnd::Start).is_none());
    assert_eq!(
        rect.base.bound_elements.as_ref().unwrap()[0].id,
        arrow.base.id
    );
}

#[test]
fn bound_arrow_follows_its_shape() {
    let mut editor = bound();
    editor.command(Command::Tool(Tool::Selection));
    // The box is transparent: select it by its top edge.
    drag(&mut editor, [50.0, 0.0], [50.0, 0.0]);
    editor.command(Command::Nudge([0.0, 20.0]));
    // Re-aimed from the start (-100, 50) at the new centre (50, 70). The box
    // has round corners (radius 25), and Excalidraw grows a rounded outline
    // by pushing each corner out along its diagonal, so the left side sits
    // gap / sqrt(2) out, not gap: x = -5 / sqrt(2), y on the aim line there.
    let [x, y] = end_point(&editor);
    let side = -5.0 / std::f64::consts::SQRT_2;
    let expected = [side, 50.0 + (100.0 + side) * 20.0 / 150.0];
    assert!(
        (x - expected[0]).abs() < 1e-6 && (y - expected[1]).abs() < 1e-6,
        "({x}, {y}) != {expected:?}"
    );
}

#[test]
fn dragging_the_arrow_away_unbinds_it() {
    let mut editor = bound();
    editor.command(Command::Tool(Tool::Selection));
    // On the body, clear of the midpoint handle at -52.5 (11 px reach).
    drag(&mut editor, [-75.0, 50.0], [-75.0, 300.0]);
    let (rect, arrow) = (&editor.scene().elements[0], &editor.scene().elements[1]);
    assert!(arrow.binding(ArrowEnd::End).is_none());
    assert!(
        rect.base
            .bound_elements
            .as_ref()
            .is_none_or(|b| b.is_empty())
    );
}

/// Real Excalidraw output. A stored end equals a re-aim only if its shape
/// moved after binding (ends placed by hand keep the release point), so this
/// checks that most shape-bound ends reproduce: 6 of 10 match within 0.02 on
/// the user's sketches, none did before rounded outlines were ported
/// exactly. Ends bound to free text are skipped (Excalidraw does not re-aim
/// them on text edits). Runs on `ROUGHDRAFT_EXTRA_FIXTURES` only.
#[test]
fn bound_ends_match_excalidraw_on_real_sketches() {
    /// A re-aimed end reproduces to float noise.
    const EXACT: f64 = 0.05;

    let Some(dir) = std::env::var_os("ROUGHDRAFT_EXTRA_FIXTURES") else {
        return;
    };
    let (mut checked, mut exact) = (0, 0);
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let memo: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let scene: Scene = serde_json::from_value(memo["excalidraw"].clone()).unwrap();
        let mut editor = Editor::new(scene);
        let before = editor.scene().elements.clone();
        let is_text = |id: &str| {
            before
                .iter()
                .any(|e| e.base.id == id && matches!(e.kind, Kind::Text(_)))
        };
        let shapes: HashSet<String> = before
            .iter()
            .filter(|e| !matches!(e.kind, Kind::Arrow(_) | Kind::Line(_)))
            .map(|e| e.base.id.clone())
            .collect();
        editor.update_bound_arrows(&shapes);
        for (old, new) in before.iter().zip(&editor.scene().elements) {
            let (Kind::Arrow(a), Kind::Arrow(b)) = (&old.kind, &new.kind) else {
                continue;
            };
            let global =
                |e: &crate::scene::Element, p: [f64; 2]| [e.base.x + p[0], e.base.y + p[1]];
            for (end, i) in [(ArrowEnd::Start, 0), (ArrowEnd::End, a.points.len() - 1)] {
                if old.binding(end).is_none_or(|b| is_text(&b.element_id)) {
                    continue;
                }
                let (p, q) = (global(old, a.points[i]), global(new, b.points[i]));
                checked += 1;
                exact += usize::from((p[0] - q[0]).hypot(p[1] - q[1]) < EXACT);
            }
        }
    }
    assert!(checked > 0, "no bound arrow ends in the extra fixtures");
    assert!(
        exact * 2 >= checked,
        "only {exact} of {checked} bound ends reproduce"
    );
}

#[test]
fn arrow_tool_drawing_and_moving_suggest_binding_targets() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Rectangle));
    drag(&mut editor, [0.0, 0.0], [100.0, 100.0]);
    let rect = editor.scene().elements[0].base.id.clone();
    let suggested = |editor: &Editor| {
        let shapes = editor.binding_suggestions();
        assert!(shapes.len() <= 1);
        shapes.first().map(|e| e.base.id.clone())
    };
    editor.command(Command::Tool(Tool::Arrow));
    assert!(editor.wants_hover());
    editor.pointer(Pointer::Hover, [104.0, 50.0], NONE);
    assert_eq!(
        suggested(&editor),
        Some(rect.clone()),
        "hovering with the arrow tool"
    );
    editor.pointer(Pointer::Hover, [-100.0, 50.0], NONE);
    assert_eq!(suggested(&editor), None);
    editor.pointer(Pointer::Down, [-100.0, 50.0], NONE);
    assert_eq!(suggested(&editor), None, "not dragged yet");
    editor.pointer(Pointer::Move, [-5.0, 50.0], NONE);
    assert_eq!(suggested(&editor), Some(rect.clone()));
    editor.pointer(Pointer::Move, [-60.0, 50.0], NONE);
    assert_eq!(suggested(&editor), None);
    editor.pointer(Pointer::Move, [-5.0, 50.0], NONE);
    editor.pointer(Pointer::Up, [-5.0, 50.0], NONE);
    assert_eq!(suggested(&editor), None, "gone once drawn");
    let arrow = &editor.scene().elements[1];
    assert_eq!(
        arrow.binding(ArrowEnd::End).map(|b| b.element_id),
        Some(rect.clone())
    );

    // Moving the arrow suggests its shape while the end stays close (away
    // from the midpoint at -52.5, which would add a point).
    editor.pointer(Pointer::Down, [-80.0, 50.0], NONE);
    editor.pointer(Pointer::Move, [-79.0, 50.0], NONE);
    assert_eq!(suggested(&editor), Some(rect));
    editor.pointer(Pointer::Move, [-180.0, 50.0], NONE);
    assert_eq!(suggested(&editor), None);
    editor.pointer(Pointer::Up, [-180.0, 50.0], NONE);
}
