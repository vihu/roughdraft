//! Copy, cut and paste.
use super::{Modifiers, drag, editor};
use crate::edit::Command;
use crate::scene::Kind;

fn select_b(editor: &mut crate::edit::Editor) {
    drag(editor, [250.0, 25.0], [250.0, 25.0], Modifiers::default());
}

#[test]
fn copy_writes_excalidraw_clipboard_json_with_labels() {
    let mut editor = editor();
    select_b(&mut editor);
    let json: serde_json::Value = serde_json::from_str(&editor.copy().unwrap()).unwrap();
    assert_eq!(json["type"], "excalidraw/clipboard");
    let ids: Vec<&str> = json["elements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["b", "t"]);
    assert!(editor.copy().is_some());
    editor.command(Command::Escape);
}

#[test]
fn paste_centres_on_the_cursor_with_fresh_ids_and_undoes() {
    let mut editor = editor();
    select_b(&mut editor);
    let json = editor.copy().unwrap();
    assert!(editor.paste(&json, [500.0, 500.0]));
    let elements = &editor.scene().elements;
    let (copy, label) = (&elements[4], &elements[5]);
    // Content box is b plus its wider label: x 200..310, y 0..50.
    assert_eq!((copy.base.x, copy.base.y), (445.0, 475.0));
    assert_ne!(copy.base.id, "b");
    assert_ne!(copy.base.seed, elements[1].base.seed);
    let Kind::Text(text) = &label.kind else {
        panic!("label")
    };
    assert_eq!(text.container_id.as_deref(), Some(copy.base.id.as_str()));
    assert!(editor.is_selected(&copy.base.id) && !editor.is_selected(&label.base.id));
    editor.command(Command::Undo);
    assert_eq!(editor.scene().elements.len(), 4);
}

#[test]
fn pasted_arrow_without_its_shape_is_unbound_and_cut_deletes() {
    let mut editor = editor();
    drag(
        &mut editor,
        [150.0, 0.0],
        [150.0, 0.0],
        Modifiers::default(),
    );
    assert!(editor.is_selected("r"));
    let json = editor.cut().unwrap();
    assert!(editor.scene().elements[3].base.is_deleted);
    assert!(editor.paste(&json, [0.0, 300.0]));
    let pasted = serde_json::to_value(editor.scene().elements.last().unwrap()).unwrap();
    assert!(pasted["endBinding"].is_null());
}

#[test]
fn paste_accepts_scene_files_and_refuses_other_text() {
    let mut editor = editor();
    assert!(!editor.paste("  ", [0.0, 0.0]));
    // JSON that is not Excalidraw's pastes as text, like any other text.
    assert!(editor.paste(r#"{"type":"other"}"#, [0.0, 0.0]));
    assert!(matches!(editor.scene().elements[4].kind, Kind::Text(_)));
    let scene = std::fs::read_to_string("tests/fixtures/scenes/l0-coverage.excalidraw").unwrap();
    assert!(editor.paste(&scene, [0.0, 0.0]));
    assert_eq!(editor.scene().elements.len(), 5 + 21);
    assert_eq!(editor.selection().count(), 20, "21 minus the label");
}

#[test]
fn plain_text_pastes_as_a_centred_text_element() {
    let mut editor = editor();
    assert!(editor.paste("hello", [100.0, 100.0]));
    let text = editor.scene().elements.last().unwrap();
    // ApproxMeasure: 5 chars * 11 = 55 wide, 25 tall.
    assert_eq!((text.base.x, text.base.y), (72.5, 87.5));
    assert!(editor.is_selected(&text.base.id));
    assert!(editor.editing().is_none());
}

#[test]
fn inserted_image_fits_600_renders_exports_and_prunes_on_save() {
    let mut editor = editor();
    let url = "data:image/png;base64,TWFu".to_owned();
    let id = editor.insert_image(url.clone(), "image/png", [1200.0, 300.0], [0.0, 0.0]);
    let image = editor.scene().elements.last().unwrap();
    assert_eq!(
        (
            image.base.x,
            image.base.y,
            image.base.width,
            image.base.height
        ),
        (-300.0, -75.0, 600.0, 150.0)
    );
    assert!(editor.is_selected(&id));
    let file_id = image.file_id().unwrap().to_owned();
    assert_eq!(editor.scene().file_data_url(&file_id), Some(url.as_str()));
    let drawing = crate::render::render_element(image, "#ffffff").unwrap();
    assert!(
        matches!(&drawing.items[..], [crate::render::Item::Image { size, .. }] if *size == [600.0, 150.0])
    );
    let svg = crate::svg::export(editor.scene(), &crate::svg::SvgOptions::default());
    assert!(
        svg.contains(r#"<image href="data:image/png;base64,TWFu""#),
        "{svg}"
    );

    editor.command(Command::Delete);
    let saved = serde_json::to_value(editor.scene().saved()).unwrap();
    assert_eq!(
        saved["files"],
        serde_json::json!({}),
        "file of a deleted image is dropped"
    );
    editor.command(Command::Undo);
    assert!(editor.scene().saved().file_data_url(&file_id).is_some());
}

#[test]
fn copied_images_carry_their_files_to_another_editor() {
    let mut from = editor();
    from.insert_image(
        "data:image/png;base64,TWFu".into(),
        "image/png",
        [10.0, 10.0],
        [0.0, 0.0],
    );
    let json = from.copy().unwrap();
    let file_id = from
        .scene()
        .elements
        .last()
        .unwrap()
        .file_id()
        .unwrap()
        .to_owned();

    let mut to = crate::edit::Editor::new(crate::scene::Scene::default());
    assert!(to.paste(&json, [100.0, 100.0]));
    assert_eq!(to.scene().elements[0].file_id(), Some(file_id.as_str()));
    assert_eq!(
        to.scene().file_data_url(&file_id),
        Some("data:image/png;base64,TWFu")
    );
}
