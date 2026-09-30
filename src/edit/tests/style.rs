//! Style panel changes.
use super::{drag, editor};
use crate::edit::{Command, Editor, Modifiers, StyleChange, Tool};
use crate::scene::{Kind, Scene};

const NONE: Modifiers = Modifiers {
    shift: false,
    alt: false,
    command: false,
};

fn select_a(editor: &mut Editor) {
    drag(editor, [50.0, 25.0], [50.0, 25.0], NONE);
}

#[test]
fn colour_applies_to_selection_and_new_elements_in_one_step() {
    let mut editor = editor();
    select_a(&mut editor);
    editor.apply_style(StyleChange::StrokeColor("#e03131".into()));
    assert_eq!(editor.scene().elements[0].base.stroke_color, "#e03131");
    assert_eq!(editor.current_style().stroke_color, "#e03131");
    editor.command(Command::Undo);
    assert_eq!(editor.scene().elements[0].base.stroke_color, "#1e1e1e");

    editor.command(Command::Tool(Tool::Rectangle));
    drag(&mut editor, [400.0, 400.0], [450.0, 450.0], NONE);
    assert_eq!(
        editor.scene().elements.last().unwrap().base.stroke_color,
        "#e03131"
    );
}

#[test]
fn sloppiness_reseeds_and_edges_only_touch_roundable_types() {
    let mut editor = editor();
    select_a(&mut editor);
    let seed = editor.scene().elements[0].base.seed;
    editor.apply_style(StyleChange::Roughness(2.0));
    let a = &editor.scene().elements[0].base;
    assert_eq!(a.roughness, 2.0);
    assert_ne!(a.seed, seed);
    editor.apply_style(StyleChange::RoundEdges(true));
    assert_eq!(
        editor.scene().elements[0]
            .base
            .roundness
            .as_ref()
            .map(|r| r.kind),
        Some(3)
    );

    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Ellipse));
    drag(&mut editor, [0.0, 0.0], [50.0, 50.0], NONE);
    let roundness = editor.scene().elements[0].base.roundness.clone();
    editor.apply_style(StyleChange::RoundEdges(false));
    assert_eq!(
        editor.scene().elements[0].base.roundness,
        roundness,
        "ellipses keep theirs"
    );
}

#[test]
fn font_size_on_a_shape_relays_out_its_label() {
    let mut editor = editor();
    drag(&mut editor, [250.0, 25.0], [250.0, 25.0], NONE);
    editor.apply_style(StyleChange::FontSize(28.0));
    let label = &editor.scene().elements[2];
    let Kind::Text(text) = &label.kind else {
        panic!("label")
    };
    assert_eq!(text.font_size, 28.0);
    // "hi" at 28px with ApproxMeasure: 2 * 28 * 0.55 = 30.8 wide, 35 tall.
    assert!((label.base.width - 30.8).abs() < 1e-9);
    assert_eq!(label.base.height, 35.0);
    assert!(
        (label.base.x - (250.0 - 15.4)).abs() < 1e-9,
        "re-centred in b"
    );
}

#[test]
fn without_a_selection_only_new_elements_change() {
    let mut editor = editor();
    let before = editor.scene().clone();
    editor.apply_style(StyleChange::BackgroundColor("#ffec99".into()));
    assert_eq!(editor.scene(), &before);
    assert_eq!(editor.current_style().background_color, "#ffec99");
}

#[test]
fn copied_styles_paste_onto_the_selection_with_fitting_roundness() {
    let mut editor = editor();
    let click = |editor: &mut crate::edit::Editor, at: [f64; 2]| {
        drag(editor, at, at, Modifiers::default());
    };
    click(&mut editor, [50.0, 25.0]);
    editor.apply_style(StyleChange::RoundEdges(true));
    editor.apply_style(StyleChange::StrokeColor("#e03131".into()));
    editor.command(Command::CopyStyles);

    click(&mut editor, [150.0, 0.0]);
    assert!(editor.is_selected("r"), "the arrow");
    editor.command(Command::PasteStyles);
    let arrow = &editor.scene().elements[3];
    assert_eq!(arrow.base.stroke_color, "#e03131");
    assert_eq!(arrow.base.background_color, "#a5d8ff");
    assert_eq!(
        arrow.base.roundness.as_ref().map(|r| r.kind),
        Some(2),
        "proportional for arrows"
    );
    editor.command(Command::Undo);
    assert_eq!(editor.scene().elements[3].base.stroke_color, "#1e1e1e");
}

#[test]
fn label_alignment_shows_and_changes_through_its_shape() {
    let mut editor = editor();
    drag(&mut editor, [250.0, 25.0], [250.0, 25.0], NONE);
    assert!(editor.is_selected("b"));
    assert_eq!(
        editor.current_style().text_align,
        crate::scene::TextAlign::Center,
        "the label's"
    );
    editor.apply_style(StyleChange::TextAlign(crate::scene::TextAlign::Left));
    let label = &editor.scene().elements[2];
    let Kind::Text(text) = &label.kind else {
        panic!("label")
    };
    assert_eq!(text.text_align, crate::scene::TextAlign::Left);
    // b spans 200..300; left-aligned labels sit 5 in from the edge.
    assert_eq!(label.base.x, 205.0);
}
