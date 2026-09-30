//! Scene and element conversions to and from JSON objects, and numbers
//! spelled the way `JSON.stringify` spells them.
use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{Base, Element, Kind, Linear, Scene, Text, restore};

impl TryFrom<Map<String, Value>> for Scene {
    type Error = serde_json::Error;

    fn try_from(mut json: Map<String, Value>) -> Result<Self, Self::Error> {
        // `take` leaves `null` in place so the key keeps its position;
        // `elements: null` is an empty scene, like `restoreElements`.
        let mut elements: Vec<Element> = match json.get_mut("elements") {
            Some(elements) if !elements.is_null() => Vec::deserialize(elements.take())?,
            _ => Vec::new(),
        };
        // A repeated id gets a new one (`restoreElements`), so ids stay unique.
        let mut seen = std::collections::HashSet::new();
        for element in &mut elements {
            while !seen.insert(element.base.id.clone()) {
                element.base.id = crate::random::id();
            }
        }
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
pub(crate) fn js_number(value: f64) -> Value {
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

impl Scene {
    /// Returns the scene as JSON text the way Excalidraw saves it
    /// (`JSON.stringify(data, null, 2)`): two-space indents and numbers
    /// spelled like JavaScript's (`0.000001`, not `1e-6`).
    pub fn to_json(&self) -> String {
        let mut out = Vec::new();
        let mut serializer = serde_json::Serializer::with_formatter(&mut out, JsFormatter::new());
        self.serialize(&mut serializer)
            .expect("scenes are plain JSON data");
        String::from_utf8(out).expect("serde_json writes UTF-8")
    }
}

/// `serde_json`'s pretty printer with JavaScript's number spelling.
struct JsFormatter<'a>(serde_json::ser::PrettyFormatter<'a>);

impl JsFormatter<'_> {
    fn new() -> Self {
        Self(serde_json::ser::PrettyFormatter::with_indent(b"  "))
    }
}

impl serde_json::ser::Formatter for JsFormatter<'_> {
    fn write_f64<W: ?Sized + std::io::Write>(
        &mut self,
        writer: &mut W,
        value: f64,
    ) -> std::io::Result<()> {
        writer.write_all(js_spelling(value).as_bytes())
    }

    fn begin_array<W: ?Sized + std::io::Write>(&mut self, writer: &mut W) -> std::io::Result<()> {
        self.0.begin_array(writer)
    }

    fn end_array<W: ?Sized + std::io::Write>(&mut self, writer: &mut W) -> std::io::Result<()> {
        self.0.end_array(writer)
    }

    fn begin_array_value<W: ?Sized + std::io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> std::io::Result<()> {
        self.0.begin_array_value(writer, first)
    }

    fn end_array_value<W: ?Sized + std::io::Write>(
        &mut self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        self.0.end_array_value(writer)
    }

    fn begin_object<W: ?Sized + std::io::Write>(&mut self, writer: &mut W) -> std::io::Result<()> {
        self.0.begin_object(writer)
    }

    fn end_object<W: ?Sized + std::io::Write>(&mut self, writer: &mut W) -> std::io::Result<()> {
        self.0.end_object(writer)
    }

    fn begin_object_key<W: ?Sized + std::io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> std::io::Result<()> {
        self.0.begin_object_key(writer, first)
    }

    fn begin_object_value<W: ?Sized + std::io::Write>(
        &mut self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        self.0.begin_object_value(writer)
    }

    fn end_object_value<W: ?Sized + std::io::Write>(
        &mut self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        self.0.end_object_value(writer)
    }
}

/// `Number.prototype.toString` for a finite number (ECMAScript
/// `Number::toString`): plain decimals from 1e-7 up to 1e21, exponents
/// outside, shortest round-trip digits, `-0` as `0`.
fn js_spelling(value: f64) -> String {
    /// Largest decimal exponent written without `e` (10^21 is the first
    /// with one).
    const MAX_PLAIN: i32 = 21;
    /// Smallest decimal exponent written without `e` (0.000001).
    const MIN_PLAIN: i32 = -6;

    if value == 0.0 {
        return "0".into();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    // Rust's `{:e}` gives the shortest digits that round-trip.
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("`{:e}` always has an exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exponent.parse::<i32>().expect("a decimal exponent") + 1;
    let body = if k <= n && n <= MAX_PLAIN {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= MAX_PLAIN {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if MIN_PLAIN < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let head = if k == 1 {
            digits
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        format!("{head}e{}{}", if e >= 0 { "+" } else { "-" }, e.abs())
    };
    format!("{sign}{body}")
}

#[cfg(test)]
mod tests {
    use super::js_spelling;

    #[test]
    fn numbers_are_spelled_like_javascript() {
        for (value, js) in [
            (0.000001, "0.000001"),
            (0.0000001, "1e-7"),
            (1.5e-7, "1.5e-7"),
            (0.1, "0.1"),
            (-0.0, "0"),
            (22.604076400856545, "22.604076400856545"),
            (1e17, "100000000000000000"),
            (9_007_199_254_740_994.0, "9007199254740994"),
            (1e21, "1e+21"),
            (1.5e21, "1.5e+21"),
            (123.456, "123.456"),
            (-35.5, "-35.5"),
        ] {
            assert_eq!(js_spelling(value), js, "{value}");
        }
    }
}
