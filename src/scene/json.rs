//! Scene and element conversions to and from JSON objects, and numbers
//! spelled the way `JSON.stringify` spells them.
use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{Base, Element, Kind, Linear, Scene, Text, restore};

impl TryFrom<Map<String, Value>> for Scene {
    type Error = serde_json::Error;

    fn try_from(mut json: Map<String, Value>) -> Result<Self, Self::Error> {
        // `take` leaves `null` in place so the key keeps its position.
        let elements = match json.get_mut("elements") {
            Some(elements) => Vec::deserialize(elements.take())?,
            None => Vec::new(),
        };
        Ok(Self { elements, json })
    }
}

impl From<Scene> for Map<String, Value> {
    fn from(scene: Scene) -> Self {
        let Scene { elements, mut json } = scene;
        let elements = elements.into_iter().map(|e| Value::Object(e.into()));
        json.insert("elements".into(), Value::Array(elements.collect()));
        json
    }
}

impl TryFrom<Map<String, Value>> for Element {
    type Error = serde_json::Error;

    fn try_from(json: Map<String, Value>) -> Result<Self, Self::Error> {
        // Typed fields come from a copy with Excalidraw's restore defaults;
        // the JSON itself stays as read.
        let typed = restore::typed_view(&json);
        let base = Base::deserialize(&typed)?;
        let kind = match typed.get("type").and_then(Value::as_str) {
            Some("rectangle") => Kind::Rectangle,
            Some("diamond") => Kind::Diamond,
            Some("ellipse") => Kind::Ellipse,
            Some("line") => Kind::Line(Linear::deserialize(&typed)?),
            Some("arrow") => Kind::Arrow(Linear::deserialize(&typed)?),
            Some("text") => Kind::Text(Text::deserialize(&typed)?),
            Some(other) => Kind::Other(other.to_owned()),
            None => return Err(serde_json::Error::missing_field("type")),
        };
        Ok(Self { base, kind, json })
    }
}

impl From<Element> for Map<String, Value> {
    fn from(element: Element) -> Self {
        let Element {
            base,
            kind,
            mut json,
        } = element;
        json.insert("type".into(), kind.type_name().into());
        write_back(&mut json, &base);
        match &kind {
            Kind::Line(linear) | Kind::Arrow(linear) => write_back(&mut json, linear),
            Kind::Text(text) => write_back(&mut json, text),
            Kind::Rectangle | Kind::Diamond | Kind::Ellipse | Kind::Other(_) => {}
        }
        json
    }
}

/// Milliseconds since the Unix epoch, like `Date.now()`.
pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

/// Overwrites `json`'s keys with `typed`'s fields, keeping key positions.
fn write_back(json: &mut Map<String, Value>, typed: &impl Serialize) {
    let fields = serde_json::to_value(typed).expect("typed fields are plain data");
    let Value::Object(fields) = fields else {
        unreachable!("typed fields serialize as a struct")
    };
    for (key, value) in fields {
        json.insert(key, js_numbers(value));
    }
}

/// A number spelled the way `JSON.stringify` spells it.
pub(super) fn js_number(value: f64) -> Value {
    js_numbers(serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number))
}

/// Spells whole floats as integers, the way `JSON.stringify` does.
pub(super) fn js_numbers(value: Value) -> Value {
    /// Largest magnitude where every integer is exact in an f64 (2^53).
    const MAX_SAFE: f64 = 9_007_199_254_740_992.0;

    match value {
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() && f.fract() == 0.0 && f.abs() < MAX_SAFE => {
                Value::from(f as i64)
            }
            _ => Value::Number(n),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(js_numbers).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(k, v)| (k, js_numbers(v)))
                .collect(),
        ),
        other => other,
    }
}
