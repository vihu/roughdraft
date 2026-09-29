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
        sketch.update(message);
    }

    let moved = &sketch.scene().elements[0].base;
    assert_eq!((moved.x, moved.y), (130.0, 110.0));
    assert!(sketch.editor().is_selected("box"));
}
