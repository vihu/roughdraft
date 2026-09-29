//! Excalidraw 0.18 scene JSON: typed where we render or edit, verbatim elsewhere.
//!
//! Every element keeps its original JSON object. Typed fields are read from it
//! on load and written back into the same keys on save, so unknown fields,
//! unknown element types and key order survive a round trip untouched. Numbers
//! written back are spelled the way `JSON.stringify` spells them (`100`, not
//! `100.0`).
use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// An Excalidraw scene: `{type, version, source, elements, appState, files}`.
///
/// Also accepts Excalidraw's clipboard payload, which has the same shape.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "Map<String, Value>", into = "Map<String, Value>")]
pub struct Scene {
    /// Elements in z-order, bottom first, including deleted tombstones.
    pub elements: Vec<Element>,
    json: Map<String, Value>,
}

/// One scene element.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "Map<String, Value>", into = "Map<String, Value>")]
pub struct Element {
    /// Fields every element type has.
    pub base: Base,
    /// Type-specific fields.
    pub kind: Kind,
    json: Map<String, Value>,
}

/// Fields every element type has.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Base {
    /// Unique id within the scene.
    pub id: String,
    /// Left edge of the unrotated bounding box, in scene units.
    pub x: f64,
    /// Top edge of the unrotated bounding box, in scene units.
    pub y: f64,
    /// Bounding box width.
    pub width: f64,
    /// Bounding box height.
    pub height: f64,
    /// Clockwise rotation in radians around the bounding box center.
    pub angle: f64,
    /// CSS color of the outline.
    pub stroke_color: String,
    /// CSS color of the fill, or `transparent`.
    pub background_color: String,
    /// How the background is filled.
    pub fill_style: FillStyle,
    /// Outline width.
    pub stroke_width: f64,
    /// Outline dash pattern.
    pub stroke_style: StrokeStyle,
    /// Corner rounding; `None` means sharp.
    pub roundness: Option<Roundness>,
    /// rough.js roughness: 0 architect, 1 artist, 2 cartoonist.
    pub roughness: f64,
    /// Opacity from 0 to 100.
    pub opacity: f64,
    /// rough.js seed. The same seed gives the same wobble as web Excalidraw.
    pub seed: i64,
    /// Deleted elements stay in the scene as tombstones.
    pub is_deleted: bool,
    /// Labels and arrows attached to this element.
    pub bound_elements: Option<Vec<BoundRef>>,
}

/// An element attached to another: its text label or a bound arrow.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct BoundRef {
    /// Id of the attached element.
    pub id: String,
    /// `text` or `arrow`.
    #[serde(rename = "type")]
    pub kind: String,
}

/// Type-specific element data.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// `rectangle`.
    Rectangle,
    /// `diamond`.
    Diamond,
    /// `ellipse`.
    Ellipse,
    /// `line`.
    Line(Linear),
    /// `arrow`.
    Arrow(Linear),
    /// `text`.
    Text(Text),
    /// Any other type (freedraw, image, frame, ...), kept verbatim.
    Other(String),
}

/// Points and arrowheads of a `line` or `arrow`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Linear {
    /// Points relative to the element's `x`/`y`; the first is usually `[0, 0]`.
    pub points: Vec<[f64; 2]>,
    /// Arrowhead at the first point.
    pub start_arrowhead: Option<Arrowhead>,
    /// Arrowhead at the last point.
    pub end_arrowhead: Option<Arrowhead>,
}

/// Content and font of a `text` element.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Text {
    /// Text as displayed, already wrapped; lines are separated by `\n`.
    pub text: String,
    /// Font size in scene units.
    pub font_size: f64,
    /// Excalidraw font id: 1 Virgil, 2 Helvetica, 3 Cascadia, 5 Excalifont,
    /// 6 Nunito, 7 Lilita One, 8 Comic Shanns, 9 Liberation Sans.
    pub font_family: u32,
    /// Horizontal alignment of each line.
    pub text_align: TextAlign,
    /// Vertical alignment inside a container.
    pub vertical_align: VerticalAlign,
    /// Unitless line height; multiply by `font_size` for scene units.
    pub line_height: f64,
    /// Shape or arrow this text is the label of.
    pub container_id: Option<String>,
}

/// Corner rounding.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Roundness {
    /// 1 legacy, 2 proportional radius, 3 adaptive radius.
    #[serde(rename = "type")]
    pub kind: u8,
    /// Fixed radius, if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
}

/// Background fill pattern.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FillStyle {
    /// Diagonal hatch lines.
    Hachure,
    /// Two crossing sets of hatch lines.
    CrossHatch,
    /// Flat fill.
    Solid,
    /// Zigzag hatch.
    Zigzag,
    /// A value this version does not know, kept verbatim.
    #[serde(untagged)]
    Other(String),
}

/// Outline dash pattern.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StrokeStyle {
    /// Continuous.
    Solid,
    /// Dashes.
    Dashed,
    /// Dots.
    Dotted,
    /// A value this version does not know, kept verbatim.
    #[serde(untagged)]
    Other(String),
}

/// Arrowhead shape.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(missing_docs)]
pub enum Arrowhead {
    Arrow,
    Bar,
    /// Legacy; Excalidraw draws it like `Circle`.
    Dot,
    Circle,
    CircleOutline,
    Triangle,
    TriangleOutline,
    Diamond,
    DiamondOutline,
    CrowfootOne,
    CrowfootMany,
    CrowfootOneOrMany,
    /// A value this version does not know, kept verbatim.
    #[serde(untagged)]
    Other(String),
}

/// Horizontal text alignment.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(missing_docs)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    /// A value this version does not know, kept verbatim.
    #[serde(untagged)]
    Other(String),
}

/// Vertical text alignment inside a container.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(missing_docs)]
pub enum VerticalAlign {
    Top,
    Middle,
    Bottom,
    /// A value this version does not know, kept verbatim.
    #[serde(untagged)]
    Other(String),
}

impl Default for Scene {
    /// An empty scene, as Excalidraw starts one.
    fn default() -> Self {
        let json = serde_json::json!({
            "type": "excalidraw",
            "version": 2,
            "source": "roughdraft",
            "elements": [],
            "appState": { "gridSize": 20, "viewBackgroundColor": "#ffffff" },
            "files": {},
        });
        serde_json::from_value(json).expect("the empty scene is valid")
    }
}

impl Scene {
    /// Returns the scene as Excalidraw saves it: deleted elements dropped and
    /// `lastCommittedPoint` cleared on lines and arrows (`serializeAsJSON`).
    #[must_use]
    pub fn saved(&self) -> Self {
        let mut scene = self.clone();
        scene.elements.retain(|e| !e.base.is_deleted);
        for element in &mut scene.elements {
            if element.json.contains_key("lastCommittedPoint") {
                element
                    .json
                    .insert("lastCommittedPoint".into(), Value::Null);
            }
        }
        scene
    }

    /// Returns `appState.viewBackgroundColor`, white when unset.
    pub fn background_color(&self) -> &str {
        self.json
            .get("appState")
            .and_then(|state| state.get("viewBackgroundColor"))
            .and_then(Value::as_str)
            .unwrap_or("#ffffff")
    }
}

impl Element {
    /// Marks the element as changed, like Excalidraw's `mutateElement`:
    /// increments `version`, draws a new `versionNonce`, sets `updated` to now.
    pub fn touch(&mut self) {
        let version = self
            .json
            .get("version")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as i64);
        self.json.insert("version".into(), (version + 1).into());
        self.json
            .insert("versionNonce".into(), crate::random::integer().into());
        self.json.insert("updated".into(), now.into());
    }

    /// Returns `(version, versionNonce)`, which changes on every [`touch`].
    ///
    /// [`touch`]: Element::touch
    pub fn revision(&self) -> (i64, i64) {
        let get = |key| self.json.get(key).and_then(Value::as_i64).unwrap_or(0);
        (get("version"), get("versionNonce"))
    }

    /// Returns a copy with a fresh `id` and `seed`, like Excalidraw's
    /// `duplicateElement`; `index` is left for Excalidraw to assign. Callers
    /// fix up references.
    #[must_use]
    pub fn duplicate(&self) -> Self {
        let mut copy = self.clone();
        copy.base.id = crate::random::id();
        copy.base.seed = crate::random::integer();
        if copy.json.contains_key("index") {
            copy.json.insert("index".into(), Value::Null);
        }
        copy.touch();
        copy
    }

    /// Drops references to `ids` (deleted elements): from `boundElements`,
    /// and arrow `startBinding`/`endBinding`. Returns whether anything changed.
    pub fn forget_bindings(&mut self, ids: &std::collections::HashSet<String>) -> bool {
        let mut changed = false;
        if let Some(bound) = &mut self.base.bound_elements {
            let before = bound.len();
            bound.retain(|b| !ids.contains(&b.id));
            changed |= bound.len() != before;
        }
        for key in ["startBinding", "endBinding"] {
            let target = self
                .json
                .get(key)
                .and_then(|b| b.get("elementId"))
                .and_then(Value::as_str);
            if target.is_some_and(|id| ids.contains(id)) {
                self.json.insert(key.into(), Value::Null);
                changed = true;
            }
        }
        changed
    }
}

impl Kind {
    /// Returns the Excalidraw `type` string.
    pub fn type_name(&self) -> &str {
        match self {
            Kind::Rectangle => "rectangle",
            Kind::Diamond => "diamond",
            Kind::Ellipse => "ellipse",
            Kind::Line(_) => "line",
            Kind::Arrow(_) => "arrow",
            Kind::Text(_) => "text",
            Kind::Other(name) => name,
        }
    }
}

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
        let json = Value::Object(json);
        let base = Base::deserialize(&json)?;
        let kind = match json.get("type").and_then(Value::as_str) {
            Some("rectangle") => Kind::Rectangle,
            Some("diamond") => Kind::Diamond,
            Some("ellipse") => Kind::Ellipse,
            Some("line") => Kind::Line(Linear::deserialize(&json)?),
            Some("arrow") => Kind::Arrow(Linear::deserialize(&json)?),
            Some("text") => Kind::Text(Text::deserialize(&json)?),
            Some(other) => Kind::Other(other.to_owned()),
            None => return Err(serde_json::Error::missing_field("type")),
        };
        let Value::Object(json) = json else {
            unreachable!("built from a map above")
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

/// Spells whole floats as integers, the way `JSON.stringify` does.
fn js_numbers(value: Value) -> Value {
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

#[cfg(test)]
mod tests {
    use super::{Arrowhead, Element, FillStyle, Kind, Scene};

    #[test]
    fn touch_and_duplicate_update_bookkeeping() {
        let mut scene: Scene = serde_json::from_str(&scene()).unwrap();
        let element = &mut scene.elements[0];
        let before = element.revision();
        element.touch();
        assert_eq!(element.revision().0, before.0 + 1);
        assert_ne!(element.revision().1, before.1);

        let copy = element.duplicate();
        assert_ne!(copy.base.id, element.base.id);
        assert_ne!(copy.base.seed, element.base.seed);
        assert_eq!(copy.revision().0, element.revision().0 + 1);
        let json = serde_json::to_value(&copy).unwrap();
        assert_eq!(json["id"], copy.base.id.as_str());
        assert_eq!(
            json["future"],
            serde_json::json!({"k": [1, 2.5]}),
            "unknown fields kept"
        );
    }

    const BASE: &str = r##""x":10,"y":20.5,"width":100,"height":50,"angle":0,"strokeColor":"#1e1e1e","backgroundColor":"transparent","fillStyle":"solid","strokeWidth":2,"strokeStyle":"solid","roughness":1,"opacity":100,"groupIds":[],"frameId":null,"roundness":null,"seed":1968410193,"version":3,"versionNonce":7,"isDeleted":false,"boundElements":null,"updated":1727654400000,"link":null,"locked":false"##;

    fn scene() -> String {
        format!(
            r##"{{"type":"excalidraw","version":2,"source":"https://excalidraw.com","elements":[{{"id":"r","type":"rectangle",{BASE},"future":{{"k":[1,2.5]}}}},{{"id":"a","type":"arrow",{BASE},"points":[[0,0],[100,50.25]],"lastCommittedPoint":null,"startBinding":null,"endBinding":null,"startArrowhead":null,"endArrowhead":"arrow","elbowed":false}},{{"id":"t","type":"text",{BASE},"text":"hi\nthere","fontSize":20,"fontFamily":5,"textAlign":"left","verticalAlign":"top","containerId":null,"originalText":"hi there","autoResize":true,"lineHeight":1.25}},{{"id":"f","type":"frame",{BASE},"name":"F1"}}],"appState":{{"gridSize":20,"viewBackgroundColor":"#ffffff"}},"files":{{}}}}"##
        )
    }

    #[test]
    fn round_trip_is_byte_identical() {
        let input = scene();
        let scene: Scene = serde_json::from_str(&input).unwrap();
        assert!(matches!(scene.elements[0].kind, Kind::Rectangle));
        assert!(matches!(&scene.elements[3].kind, Kind::Other(name) if name == "frame"));
        let Kind::Arrow(arrow) = &scene.elements[1].kind else {
            panic!("expected an arrow");
        };
        assert_eq!(arrow.end_arrowhead, Some(Arrowhead::Arrow));
        assert_eq!(serde_json::to_string(&scene).unwrap(), input);
    }

    #[test]
    fn edits_are_written_in_place_as_js_numbers() {
        let mut scene: Scene = serde_json::from_str(&scene()).unwrap();
        scene.elements[0].base.x = 42.0;
        scene.elements[0].base.width = 99.5;
        let out = serde_json::to_string(&scene).unwrap();
        assert!(out.contains(r#""type":"rectangle","x":42,"y":20.5,"width":99.5,"#));
    }

    #[test]
    fn unknown_enum_values_round_trip() {
        let input = scene().replacen(r#""fillStyle":"solid""#, r#""fillStyle":"dots""#, 1);
        let scene: Scene = serde_json::from_str(&input).unwrap();
        assert_eq!(
            scene.elements[0].base.fill_style,
            FillStyle::Other("dots".into())
        );
        assert_eq!(serde_json::to_string(&scene).unwrap(), input);
    }

    #[test]
    fn missing_required_field_is_rejected() {
        let input = format!(r#"{{"id":"r","type":"rectangle",{BASE}}}"#)
            .replace(r#""seed":1968410193,"#, "");
        let err = serde_json::from_str::<Element>(&input).unwrap_err();
        assert!(err.to_string().contains("missing field `seed`"), "{err}");
    }

    #[test]
    fn missing_type_is_rejected() {
        let input = format!(r#"{{"id":"r",{BASE}}}"#);
        let err = serde_json::from_str::<Element>(&input).unwrap_err();
        assert!(err.to_string().contains("missing field `type`"), "{err}");
    }
}
