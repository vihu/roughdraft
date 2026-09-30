//! Z-order and groups.
use super::{SHIFT, drag, editor};
use crate::edit::{Command, Editor, Modifiers, Order};

const NONE: Modifiers = Modifiers {
    shift: false,
    alt: false,
    command: false,
};

fn ids(editor: &Editor) -> Vec<&str> {
    editor
        .scene()
        .elements
        .iter()
        .map(|e| e.base.id.as_str())
        .collect()
}

fn click(editor: &mut Editor, at: [f64; 2], modifiers: Modifiers) {
    drag(editor, at, at, modifiers);
}

#[test]
fn one_step_moves_past_the_next_shape_and_its_label() {
    let mut editor = editor();
    click(&mut editor, [50.0, 25.0], NONE);
    editor.command(Command::Reorder(Order::Forward));
    assert_eq!(ids(&editor), ["b", "t", "a", "r"]);
    editor.command(Command::Reorder(Order::ToFront));
    assert_eq!(ids(&editor), ["b", "t", "r", "a"]);
    editor.command(Command::Reorder(Order::ToBack));
    assert_eq!(ids(&editor), ["a", "b", "t", "r"]);
    editor.command(Command::Undo);
    assert_eq!(ids(&editor), ["b", "t", "r", "a"]);
}

#[test]
fn a_selected_run_keeps_its_order() {
    let mut editor = editor();
    click(&mut editor, [50.0, 25.0], NONE);
    click(&mut editor, [250.0, 25.0], SHIFT);
    editor.command(Command::Reorder(Order::Forward));
    assert_eq!(ids(&editor), ["r", "a", "b", "t"]);
    editor.command(Command::Reorder(Order::Backward));
    assert_eq!(ids(&editor), ["a", "b", "t", "r"]);
}

#[test]
fn grouped_elements_select_together_and_double_click_enters_the_group() {
    let mut editor = editor();
    click(&mut editor, [50.0, 25.0], NONE);
    click(&mut editor, [250.0, 25.0], SHIFT);
    editor.command(Command::Group);
    let group = editor.scene().elements[0].group_ids()[0].to_owned();
    assert_eq!(
        editor.scene().elements[2].group_ids(),
        [group.as_str()],
        "the label joins too"
    );

    click(&mut editor, [600.0, 600.0], NONE);
    click(&mut editor, [50.0, 25.0], NONE);
    assert!(
        editor.is_selected("a") && editor.is_selected("b"),
        "one click selects the group"
    );
    assert_eq!(editor.selected_groups().len(), 1);
    assert!(editor.in_selected_group("a"));

    editor.double_click([50.0, 25.0]);
    assert!(
        editor.is_selected("a") && !editor.is_selected("b"),
        "inside the group"
    );
    click(&mut editor, [600.0, 600.0], NONE);
    click(&mut editor, [250.0, 25.0], NONE);
    assert!(
        editor.is_selected("a"),
        "leaving the group selects it whole again"
    );

    editor.command(Command::Ungroup);
    assert!(editor.scene().elements[0].group_ids().is_empty());
    editor.command(Command::Undo);
    assert_eq!(editor.scene().elements[0].group_ids(), [group.as_str()]);
}
