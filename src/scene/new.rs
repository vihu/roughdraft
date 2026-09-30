//! New elements with Excalidraw's defaults (`element/newElement.ts`).
use serde_json::{Map, Value, json};

use super::{Base, Element, Kind};

/// Base keys in the order Excalidraw writes them.
const BASE_KEYS: [&str; 26] = [
    "id",
    "type",
    "x",
    "y",
    "width",
    "height",
    "angle",
    "strokeColor",
    "backgroundColor",
    "fillStyle",
    "strokeWidth",
    "strokeStyle",
    "roughness",
    "opacity",
    "groupIds",
    "frameId",
    "index",
    "roundness",
    "seed",
    "version",
    "versionNonce",
    "isDeleted",
    "boundElements",
    "updated",
    "link",
    "locked",
];

impl Element {
    /// Creates an element from typed fields, adding what Excalidraw gives
    /// every new element: no groups or frame, `index` null (Excalidraw
    /// assigns it), version 1, a random `versionNonce`, `updated` now, no
    /// link, unlocked. Lines and arrows get no bindings, arrows
    /// `elbowed: false`; text gets `originalText` and `autoResize: true`.
    pub fn new(kind: Kind, base: Base) -> Self {
        let mut json: Map<String, Value> = BASE_KEYS
            .iter()
            .map(|k| ((*k).to_owned(), Value::Null))
            .collect();
        json.insert("groupIds".into(), json!([]));
        json.insert("version".into(), 1.into());
        json.insert("versionNonce".into(), crate::random::integer().into());
        json.insert("updated".into(), super::now_ms().into());
        json.insert("locked".into(), false.into());
        match &kind {
            Kind::Line(_) | Kind::Arrow(_) => {
                for key in [
                    "points",
                    "lastCommittedPoint",
                    "startBinding",
                    "endBinding",
                    "startArrowhead",
                    "endArrowhead",
                ] {
                    json.insert(key.into(), Value::Null);
                }
                if matches!(kind, Kind::Arrow(_)) {
                    json.insert("elbowed".into(), false.into());
                }
            }
            Kind::Text(text) => {
                for key in [
                    "text",
                    "fontSize",
                    "fontFamily",
                    "textAlign",
                    "verticalAlign",
                    "containerId",
                ] {
                    json.insert(key.into(), Value::Null);
                }
                json.insert("originalText".into(), text.text.clone().into());
                json.insert("autoResize".into(), true.into());
                json.insert("lineHeight".into(), Value::Null);
            }
            Kind::Rectangle | Kind::Diamond | Kind::Ellipse | Kind::Other(_) => {}
        }
        // Writing through the JSON form fills the placeholders in place.
        let map: Map<String, Value> = Element { base, kind, json }.into();
        Element::try_from(map).expect("typed fields form a valid element")
    }
}

#[cfg(test)]
mod tests {
    use crate::scene::{Arrowhead, Base, Element, FillStyle, Kind, Linear, StrokeStyle};

    #[test]
    fn new_arrow_has_every_field_excalidraw_writes() {
        let base = Base {
            id: "a".into(),
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 0.0,
            angle: 0.0,
            stroke_color: "#1e1e1e".into(),
            background_color: "transparent".into(),
            fill_style: FillStyle::Solid,
            stroke_width: 2.0,
            stroke_style: StrokeStyle::Solid,
            roundness: None,
            roughness: 1.0,
            opacity: 100.0,
            seed: 7,
            is_deleted: false,
            bound_elements: None,
        };
        let linear = Linear {
            points: vec![[0.0, 0.0], [100.0, 0.0]],
            start_arrowhead: None,
            end_arrowhead: Some(Arrowhead::Arrow),
        };
        let json = serde_json::to_value(Element::new(Kind::Arrow(linear), base)).unwrap();
        let keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(&keys[..4], ["id", "type", "x", "y"]);
        assert_eq!(keys.len(), 26 + 7);
        assert_eq!(json["type"], "arrow");
        assert_eq!(json["x"], 10);
        assert_eq!(json["points"], serde_json::json!([[0, 0], [100, 0]]));
        assert_eq!(json["endArrowhead"], "arrow");
        assert_eq!(json["elbowed"], false);
        assert_eq!(json["version"], 1);
        assert!(json["index"].is_null() && json["startBinding"].is_null());
    }
}
