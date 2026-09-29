//! Loading and saving a fixture scene must not change its JSON.
//!
//! Compared as JSON values: formatting and key order may differ from a
//! pretty-printed fixture, values may not.
mod common;

use roughdraft::scene::Scene;

#[test]
fn fixtures_round_trip_unchanged() {
    for fixture in common::fixtures() {
        let scene: Scene = serde_json::from_value(fixture.scene.clone()).unwrap();
        let saved = serde_json::to_value(&scene).unwrap();
        assert_eq!(saved, fixture.scene, "{}", fixture.name);
    }
}
