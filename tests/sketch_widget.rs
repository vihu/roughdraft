//! The canvas turns pointer input into editor messages at the right scene
//! coordinates, driven through iced's headless simulator.
use iced::mouse::{self, Button};
use iced::{Event, Point};
use roughdraft::scene::Scene;
use roughdraft::widget::Sketch;

#[test]
fn dragging_on_the_canvas_moves_the_element_under_the_pointer() {
    let json = r##"{"type":"excalidraw","elements":[{"id":"box","type":"rectangle","x":100,"y":100,"width":160,"height":90,"angle":0,"strokeColor":"#1e1e1e","backgroundColor":"#a5d8ff","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roundness":null,"roughness":1,"opacity":100,"seed":1,"version":1,"versionNonce":1,"isDeleted":false,"boundElements":null}]}"##;
    let mut sketch = Sketch::new(serde_json::from_str::<Scene>(json).unwrap());
    // Scene point (50, 50) at the canvas' top-left corner.
    sketch.set_origin([50.0, 50.0]);

    let messages: Vec<_> = {
        let mut ui = iced_test::Simulator::with_size(
            iced::Settings::default(),
            (800.0, 600.0),
            sketch.view(),
        );
        // Hovering must neither pan nor publish.
        for hover in [Point::new(10.0, 10.0), Point::new(100.0, 100.0)] {
            ui.point_at(hover);
            ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: hover })]);
        }
        // Screen (100, 100) is scene (150, 150): inside the box.
        ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(Button::Left))]);
        let to = Point::new(130.0, 110.0);
        ui.point_at(to);
        ui.simulate([Event::Mouse(mouse::Event::CursorMoved { position: to })]);
        ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(Button::Left))]);
        ui.into_messages().collect()
    };
    assert_eq!(messages.len(), 3, "down, move, up");
    for message in messages {
        let _ = sketch.update(message);
    }

    let moved = &sketch.scene().elements[0].base;
    assert_eq!((moved.x, moved.y), (130.0, 110.0));
    assert!(sketch.editor().is_selected("box"));
}

/// Feeds one round of UI events and applies the messages they produce
/// (tasks, such as focusing, are not run).
fn run(
    sketch: &mut Sketch,
    events: impl FnOnce(&mut iced_test::Simulator<'_, roughdraft::widget::Message>),
) {
    let messages: Vec<_> = {
        let settings = iced::Settings {
            fonts: vec![roughdraft::widget::EXCALIFONT.into()],
            ..iced::Settings::default()
        };
        let mut ui = iced_test::Simulator::with_size(settings, (800.0, 600.0), sketch.view());
        events(&mut ui);
        ui.into_messages().collect()
    };
    for message in messages {
        let _ = sketch.update(message);
    }
}

#[test]
fn double_click_then_typing_creates_measured_text() {
    let mut sketch = Sketch::new(Scene::default());
    sketch.set_origin([0.0, 0.0]);
    let at = Point::new(100.0, 100.0);
    run(&mut sketch, |ui| {
        ui.point_at(at);
        for _ in 0..2 {
            ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(Button::Left))]);
            ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(Button::Left))]);
        }
    });
    assert!(
        sketch.editor().editing().is_some(),
        "double-click starts text"
    );

    run(&mut sketch, |ui| {
        // Focus the overlay (the focus task is not run here), then type.
        ui.point_at(Point::new(102.0, 110.0));
        ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(Button::Left))]);
        ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(Button::Left))]);
        ui.typewrite("hi");
    });
    run(&mut sketch, |ui| {
        ui.point_at(Point::new(102.0, 110.0));
        ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(Button::Left))]);
        ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(Button::Left))]);
        let _ = ui.tap_key(iced::keyboard::key::Named::Escape);
    });

    assert!(sketch.editor().editing().is_none(), "Escape finishes");
    let text = &sketch.scene().elements[0];
    let json = serde_json::to_value(text).unwrap();
    assert_eq!(
        (json["type"].as_str(), json["text"].as_str()),
        (Some("text"), Some("hi"))
    );
    // Excalifont "hi" at 20px is about 15 wide (the fallback guess is 22).
    let width = text.base.width;
    assert!(width > 5.0 && width < 21.0, "measured width {width}");
    assert_eq!((text.base.x, text.base.y), (100.0, 100.0));
}

#[test]
fn clicking_a_swatch_restyles_the_selection_without_deselecting() {
    let json = r##"{"type":"excalidraw","elements":[{"id":"box","type":"rectangle","x":400,"y":300,"width":160,"height":90,"angle":0,"strokeColor":"#1e1e1e","backgroundColor":"#a5d8ff","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roundness":null,"roughness":1,"opacity":100,"seed":1,"version":1,"versionNonce":1,"isDeleted":false,"boundElements":null}]}"##;
    let mut sketch = Sketch::new(serde_json::from_str::<Scene>(json).unwrap());
    sketch.set_origin([0.0, 0.0]);
    let click = |at: Point| {
        move |ui: &mut iced_test::Simulator<'_, roughdraft::widget::Message>| {
            ui.point_at(at);
            ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(Button::Left))]);
            ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(Button::Left))]);
        }
    };
    run(&mut sketch, click(Point::new(480.0, 345.0)));
    assert!(sketch.editor().is_selected("box"));
    // Second stroke swatch (red): panel at (12, 76), padding 12, label, 22px swatches 4 apart.
    run(&mut sketch, click(Point::new(61.0, 117.0)));
    assert!(
        sketch.editor().is_selected("box"),
        "the click stays in the panel"
    );
    assert_eq!(sketch.scene().elements[0].base.stroke_color, "#e03131");
}

#[test]
fn inserted_image_file_lands_centred_in_the_view_as_a_data_url() {
    let mut png = Vec::new();
    image::RgbaImage::new(40, 20)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let mut sketch = Sketch::new(Scene::default());
    sketch.set_origin([0.0, 0.0]);
    assert!(sketch.insert_image(b"not an image").is_err());
    sketch.insert_image(&png).unwrap();

    let image = sketch.scene().elements.last().unwrap();
    // The default 800x600 view is centred on scene (400, 300).
    assert_eq!(
        (
            image.base.x,
            image.base.y,
            image.base.width,
            image.base.height
        ),
        (380.0, 290.0, 40.0, 20.0)
    );
    let url = sketch
        .scene()
        .file_data_url(image.file_id().unwrap())
        .unwrap();
    assert!(url.starts_with("data:image/png;base64,iVBOR"), "{url}");

    // A picture taller than half the 600 px view is shown 300 high.
    png.clear();
    image::RgbaImage::new(1000, 800)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    sketch.insert_image(&png).unwrap();
    let image = sketch.scene().elements.last().unwrap();
    assert_eq!((image.base.width, image.base.height), (375.0, 300.0));
}

#[test]
fn typing_a_hex_colour_restyles_the_selection() {
    use iced::keyboard::{Key, key::Named};

    let json = r##"{"type":"excalidraw","elements":[{"id":"box","type":"rectangle","x":400,"y":300,"width":160,"height":90,"angle":0,"strokeColor":"#1e1e1e","backgroundColor":"#a5d8ff","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roundness":null,"roughness":1,"opacity":100,"seed":1,"version":1,"versionNonce":1,"isDeleted":false,"boundElements":null}]}"##;
    let mut sketch = Sketch::new(serde_json::from_str::<Scene>(json).unwrap());
    sketch.set_origin([0.0, 0.0]);
    run(&mut sketch, |ui| {
        ui.point_at(Point::new(480.0, 345.0));
        ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(Button::Left))]);
        ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(Button::Left))]);
    });
    run(&mut sketch, |ui| {
        ui.click("#1e1e1e")
            .expect("the stroke field shows the colour");
        ui.tap_key(Key::Named(Named::End));
        for _ in 0..6 {
            ui.tap_key(Key::Named(Named::Backspace));
        }
        // Each whole colour on the way applies, like Excalidraw's field;
        // "#e0313" is not one, so it waits for the last digit.
        ui.typewrite("e03131");
    });
    assert!(sketch.editor().is_selected("box"));
    assert_eq!(sketch.scene().elements[0].base.stroke_color, "#e03131");
}

#[test]
fn footer_zooms_and_undoes() {
    let json = r##"{"type":"excalidraw","elements":[{"id":"box","type":"rectangle","x":400,"y":300,"width":160,"height":90,"angle":0,"strokeColor":"#1e1e1e","backgroundColor":"#a5d8ff","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roundness":null,"roughness":1,"opacity":100,"seed":1,"version":1,"versionNonce":1,"isDeleted":false,"boundElements":null}]}"##;
    let mut sketch = Sketch::new(serde_json::from_str::<Scene>(json).unwrap());
    run(&mut sketch, |ui| {
        ui.click("+").expect("zoom in");
    });
    assert!((sketch.zoom() - 1.1).abs() < 1e-9);
    run(&mut sketch, |ui| {
        ui.click("110%").expect("the level resets");
    });
    assert_eq!(sketch.zoom(), 1.0);

    sketch.set_origin([0.0, 0.0]);
    run(&mut sketch, |ui| {
        let at = Point::new(480.0, 345.0);
        ui.point_at(at);
        ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(Button::Left))]);
        ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(Button::Left))]);
    });
    let moved = sketch.scene().elements[0].base.x;
    run(&mut sketch, |ui| {
        ui.tap_key(iced::keyboard::Key::Named(
            iced::keyboard::key::Named::ArrowRight,
        ));
    });
    assert_eq!(sketch.scene().elements[0].base.x, moved + 1.0);
    run(&mut sketch, |ui| {
        ui.click("Undo").expect("undo button");
    });
    assert_eq!(sketch.scene().elements[0].base.x, moved);
}

#[test]
fn keys_after_a_press_outside_the_canvas_belong_to_that_widget() {
    // A host puts the sketch under its own widgets (a title field, say).
    let json = r##"{"type":"excalidraw","elements":[{"id":"box","type":"rectangle","x":400,"y":300,"width":160,"height":90,"angle":0,"strokeColor":"#1e1e1e","backgroundColor":"#a5d8ff","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roundness":null,"roughness":1,"opacity":100,"seed":1,"version":1,"versionNonce":1,"isDeleted":false,"boundElements":null}]}"##;
    let mut sketch = Sketch::new(serde_json::from_str::<Scene>(json).unwrap());
    sketch.set_origin([0.0, 0.0]);
    let host = |sketch: &mut Sketch, presses: &[Point]| {
        let messages: Vec<_> = {
            let view = iced::widget::column![iced::widget::space().height(100), sketch.canvas()];
            let mut ui =
                iced_test::Simulator::with_size(iced::Settings::default(), (800.0, 700.0), view);
            for &at in presses {
                ui.point_at(at);
                ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(Button::Left))]);
                ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(Button::Left))]);
            }
            let _ = ui.tap_key(iced::keyboard::key::Named::Backspace);
            ui.into_messages().collect()
        };
        for message in messages {
            let _ = sketch.update(message);
        }
    };
    // The box is at screen (480, 445), under the 100 px space. Selected,
    // then a press above the canvas: Backspace is the other widget's.
    host(
        &mut sketch,
        &[Point::new(480.0, 445.0), Point::new(480.0, 50.0)],
    );
    assert!(sketch.editor().is_selected("box"));
    assert!(!sketch.scene().elements[0].base.is_deleted);
    // With the canvas pressed last, it deletes.
    host(&mut sketch, &[Point::new(480.0, 445.0)]);
    assert!(sketch.scene().elements[0].base.is_deleted);
}

#[test]
fn the_wheel_scrolls_like_iced_scrollables() {
    let json = r##"{"type":"excalidraw","elements":[{"id":"box","type":"rectangle","x":400,"y":300,"width":160,"height":90,"angle":0,"strokeColor":"#1e1e1e","backgroundColor":"#a5d8ff","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roundness":null,"roughness":1,"opacity":100,"seed":1,"version":1,"versionNonce":1,"isDeleted":false,"boundElements":null}]}"##;
    let mut sketch = Sketch::new(serde_json::from_str::<Scene>(json).unwrap());
    sketch.set_origin([0.0, 0.0]);
    // Wheel up: the content moves down 50, so the box's old top edge on
    // screen is now above it.
    run(&mut sketch, |ui| {
        ui.point_at(Point::new(480.0, 345.0));
        ui.simulate([Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
        })]);
    });
    run(&mut sketch, |ui| {
        ui.point_at(Point::new(480.0, 305.0));
        ui.simulate([Event::Mouse(mouse::Event::ButtonPressed(Button::Left))]);
        ui.simulate([Event::Mouse(mouse::Event::ButtonReleased(Button::Left))]);
    });
    assert!(!sketch.editor().is_selected("box"));
}
