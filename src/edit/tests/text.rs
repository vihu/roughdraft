//! Text elements and labels.
use super::{drag, editor};
use crate::edit::{Command, Editor, Modifiers, Pointer, Tool};
use crate::scene::{Kind, Scene};

const NONE: Modifiers = Modifiers {
    shift: false,
    alt: false,
    command: false,
};

fn click(editor: &mut Editor, at: [f64; 2]) {
    editor.pointer(Pointer::Down, at, NONE);
    editor.pointer(Pointer::Up, at, NONE);
}

fn text_of(editor: &Editor) -> String {
    match &editor.editing().expect("editing").kind {
        Kind::Text(text) => text.text.clone(),
        _ => unreachable!(),
    }
}

#[test]
fn text_tool_click_types_free_text_sized_by_the_measure() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Text));
    click(&mut editor, [10.0, 20.0]);
    assert_eq!(editor.tool(), Tool::Selection, "the tool reverts on press");
    editor.set_text("hello\nhi");
    // ApproxMeasure: 11 units per char at 20px; 2 lines of 25.
    let text = editor.editing().unwrap().base.clone();
    assert_eq!(
        (text.x, text.y, text.width, text.height),
        (10.0, 20.0, 55.0, 50.0)
    );
    let id = text.id.clone();
    editor.command(Command::Escape);
    assert!(editor.editing().is_none());
    assert!(editor.is_selected(&id));
    editor.command(Command::Undo);
    assert!(
        editor.scene().elements.is_empty(),
        "typing is one undo step"
    );
}

#[test]
fn empty_text_is_deleted_when_editing_ends() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Text));
    click(&mut editor, [0.0, 0.0]);
    editor.set_text("   ");
    click(&mut editor, [300.0, 300.0]);
    assert!(editor.scene().saved().elements.is_empty());
}

#[test]
fn text_tool_on_a_shape_adds_a_wrapped_label_and_grows_it() {
    let mut editor = editor();
    editor.command(Command::Tool(Tool::Text));
    click(&mut editor, [50.0, 25.0]);
    editor.set_text("hello world foo");
    assert_eq!(text_of(&editor), "hello\nworld\nfoo", "wrapped to 100 - 10");
    let label = editor.editing().unwrap().clone();
    let Kind::Text(text) = &label.kind else {
        unreachable!()
    };
    assert_eq!(text.container_id.as_deref(), Some("a"));
    let a = &editor.scene().elements[0].base;
    assert_eq!(a.height, 85.0, "3 lines of 25 plus padding");
    assert_eq!(a.bound_elements.as_ref().unwrap()[0].id, label.base.id);
    // Centred in the grown box: x (100 - 55) / 2, y (85 - 75) / 2.
    assert_eq!((label.base.x, label.base.y), (22.5, 5.0));
    let json = serde_json::to_value(&label).unwrap();
    assert_eq!(json["originalText"], "hello world foo");
}

#[test]
fn double_click_and_enter_edit_existing_text() {
    let mut editor = editor();
    editor.double_click([250.0, 25.0]);
    assert_eq!(editor.editing().unwrap().base.id, "t", "the box's label");
    editor.command(Command::Escape);
    assert!(editor.is_selected("b"));

    drag(&mut editor, [50.0, 25.0], [50.0, 25.0], NONE);
    editor.command(Command::Finish);
    assert!(
        editor.editing().is_some(),
        "Enter on a shape edits its (new) label"
    );

    editor.double_click([600.0, 600.0]);
    assert_eq!(
        editor.editing().unwrap().base.x,
        600.0,
        "double-click on nothing starts text"
    );
}

#[test]
fn centred_text_grows_around_its_centre() {
    let mut editor = Editor::new(Scene::default());
    let mut style = editor.style().clone();
    style.text_align = crate::scene::TextAlign::Center;
    editor.set_style(style);
    editor.command(Command::Tool(Tool::Text));
    click(&mut editor, [100.0, 0.0]);
    editor.set_text("ab");
    let x = editor.editing().unwrap().base.x;
    assert_eq!(x, 100.0 - 11.0);
}

#[test]
fn arrow_label_sits_on_the_middle_point_not_the_box_centre() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Arrow));
    for at in [[0.0, 200.0], [100.0, 200.0], [100.0, 300.0]] {
        editor.pointer(Pointer::Move, at, NONE);
        click(&mut editor, at);
    }
    editor.command(Command::Finish);
    editor.double_click([50.0, 200.0]);
    editor.set_text("hi");
    editor.finish_text();
    let label = editor.scene().elements.last().unwrap();
    // ApproxMeasure: 22 x 25, centred on the corner point (100, 200).
    assert_eq!((label.base.x, label.base.y), (89.0, 187.5));
}

#[test]
fn side_resize_wraps_free_text_and_later_edits_keep_the_width() {
    let mut editor = Editor::new(Scene::default());
    editor.command(Command::Tool(Tool::Text));
    click(&mut editor, [300.0, 300.0]);
    editor.set_text("hello world foo");
    editor.finish_text();
    editor.command(Command::Tool(Tool::Selection));
    let text = editor.scene().elements[0].clone();
    let (x, y, w, h) = (text.base.x, text.base.y, text.base.width, text.base.height);
    assert_eq!((w, h), (165.0, 25.0), "ApproxMeasure: 15 chars");
    click(&mut editor, [x + 20.0, y + h / 2.0]);
    let right = [x + w + 4.0, y + h / 2.0];
    assert_eq!(editor.handle_at(right), Some(crate::edit::Handle::E));
    drag(&mut editor, right, [x + 74.0, y + h / 2.0], NONE);

    let text = &editor.scene().elements[0];
    let Kind::Text(body) = &text.kind else {
        unreachable!()
    };
    assert_eq!(body.text, "hello\nworld\nfoo");
    assert_eq!(body.font_size, 20.0, "the font keeps its size");
    assert_eq!(
        (text.base.x, text.base.y, text.base.width, text.base.height),
        (x, y, 70.0, 75.0)
    );
    assert!(!text.auto_resize());

    let id = text.base.id.clone();
    editor.double_click([x + 10.0, y + 10.0]);
    assert_eq!(editor.editing().map(|e| e.base.id.clone()), Some(id));
    editor.set_text("hello world foo bar");
    assert_eq!(text_of(&editor), "hello\nworld\nfoo\nbar");
    assert_eq!(editor.editing().unwrap().base.width, 70.0);
}

#[test]
fn font_size_steps_by_ten_percent_rounded() {
    let mut editor = editor();
    click(&mut editor, [250.0, 25.0]);
    let size = |editor: &Editor| match &editor.scene().elements[2].kind {
        Kind::Text(text) => text.font_size,
        _ => unreachable!(),
    };
    editor.command(Command::LargerFont);
    editor.command(Command::LargerFont);
    assert_eq!(size(&editor), 24.0, "20 -> 22 -> round(24.2)");
    editor.command(Command::SmallerFont);
    assert_eq!(size(&editor), 22.0, "round(24 / 1.1)");
    assert_eq!(editor.style().font_size, 22.0);
    editor.command(Command::Undo);
    assert_eq!(size(&editor), 24.0);
}
