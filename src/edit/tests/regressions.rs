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
