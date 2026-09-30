//! Defaults for fields older or hand-written scenes leave out, the way
//! Excalidraw's `restoreElement` fills them (`data/restore.ts`). Parsing
//! uses them; saving then writes them like Excalidraw's own save does.
//! Complete 0.18 scenes are unaffected and still round-trip byte for byte.
use serde_json::{Map, Value, json};

/// `ROUNDNESS.LEGACY`, for shapes that now use the adaptive radius.
const LEGACY: u8 = 1;

/// `ROUNDNESS.PROPORTIONAL_RADIUS`.
const PROPORTIONAL: u8 = 2;

/// Returns `json` with Excalidraw's restore defaults filled in, for parsing
/// the typed fields.
pub(super) fn typed_view(json: &Map<String, Value>) -> Value {
    let mut typed = json.clone();
    let kind = json.get("type").and_then(Value::as_str).unwrap_or_default();
    // `||` in Excalidraw: any falsy value takes the default.
    for (key, value) in [
        ("fillStyle", json!("solid")),
        ("strokeWidth", json!(2)),
        ("strokeColor", json!("#1e1e1e")),
        ("backgroundColor", json!("transparent")),
        ("angle", json!(0)),
        ("width", json!(0)),
        ("height", json!(0)),
    ] {
        if typed.get(key).is_none_or(is_falsy) {
            typed.insert(key.into(), value);
        }
    }
    // `??`: only a missing or null value takes the default.
    for (key, value) in [
        ("x", json!(0)),
        ("y", json!(0)),
        ("strokeStyle", json!("solid")),
        ("roughness", json!(1)),
        ("opacity", json!(100)),
        ("seed", json!(1)),
        ("isDeleted", json!(false)),
    ] {
        if typed.get(key).is_none_or(Value::is_null) {
            typed.insert(key.into(), value);
        }
    }
    if typed.get("roundness").is_none_or(is_falsy)
        && json.get("strokeSharpness").and_then(Value::as_str) == Some("round")
    {
        let adaptive = matches!(kind, "rectangle" | "embeddable" | "iframe" | "image");
        let kind = if adaptive { LEGACY } else { PROPORTIONAL };
        typed.insert("roundness".into(), json!({ "type": kind }));
    }
    if let Some(Value::Array(ids)) = json.get("boundElementIds") {
        let bound: Vec<Value> = ids
            .iter()
            .map(|id| json!({ "type": "arrow", "id": id }))
            .collect();
        typed.insert("boundElements".into(), Value::Array(bound));
    }
    match kind {
        "text" => restore_text(&mut typed),
        "line" | "arrow" if typed.get("points").is_none_or(|p| !p.is_array()) => {
            let (w, h) = (&typed["width"], &typed["height"]);
            let points = json!([[0, 0], [w, h]]);
            typed.insert("points".into(), points);
        }
        _ => {}
    }
    Value::Object(typed)
}

/// Excalidraw's unitless line height for a font family (`getLineHeight`).
pub(crate) fn line_height(font_family: u32) -> f64 {
    match font_family {
        6 => 1.35,
        3 => 1.2,
        2 | 7 | 9 => 1.15,
        _ => 1.25,
    }
}

fn restore_text(typed: &mut Map<String, Value>) {
    for (key, value) in [
        ("text", json!("")),
        ("textAlign", json!("left")),
        ("verticalAlign", json!("top")),
    ] {
        if typed.get(key).is_none_or(is_falsy) {
            typed.insert(key.into(), value);
        }
    }
    for (key, value) in [("fontSize", json!(20)), ("fontFamily", json!(5))] {
        if typed.get(key).is_none_or(Value::is_null) {
            typed.insert(key.into(), value);
        }
    }
    if typed.get("lineHeight").is_none_or(is_falsy) {
        let number = |key: &str| typed.get(key).and_then(Value::as_f64).unwrap_or(0.0);
        let (height, size) = (number("height"), number("fontSize"));
        let lines = typed["text"]
            .as_str()
            .unwrap_or_default()
            .split('\n')
            .count() as f64;
        // Old diagrams: the line height their stored height implies
        // (`detectLineHeight`); otherwise the family's.
        let family = typed["fontFamily"].as_u64().unwrap_or(5) as u32;
        let line_height = if height > 0.0 && size > 0.0 {
            height / lines / size
        } else {
            line_height(family)
        };
        typed.insert("lineHeight".into(), json!(line_height));
    }
}

/// JavaScript truthiness for JSON values.
fn is_falsy(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        Value::Array(_) | Value::Object(_) => false,
    }
}
