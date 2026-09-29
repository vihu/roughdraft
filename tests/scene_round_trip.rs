//! Loading and saving a fixture scene must not change its JSON.
//!
//! Compared as JSON values: formatting and key order may differ from a
//! pretty-printed fixture, values may not.
use std::path::Path;

use roughdraft::scene::Scene;
use serde_json::Value;

#[test]
fn fixtures_round_trip_unchanged() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scenes");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "excalidraw") {
            continue;
        }
        let original: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let scene: Scene = serde_json::from_value(original.clone()).unwrap();
        let saved = serde_json::to_value(&scene).unwrap();
        assert_eq!(saved, original, "{}", path.display());
        checked += 1;
    }
    assert!(checked > 0, "no fixtures in {}", dir.display());
}
