//! Fixture loading shared by the scene tests.
//!
//! Always reads `tests/fixtures/scenes/<name>.excalidraw` (+ `<name>.svg`).
//! When `ROUGHDRAFT_EXTRA_FIXTURES` names a directory, also reads the Keeprs
//! sketch memos in it (`{"excalidraw": scene, "svg": ...}` per `.json` file),
//! so private sketches can be checked without committing them.
// Each test binary uses a different subset of these items.
#![allow(dead_code)]
use std::path::{Path, PathBuf};

use serde_json::Value;

/// A scene and, when there is one, Excalidraw's SVG export of it.
pub struct Fixture {
    pub name: String,
    pub scene: Value,
    pub svg: Option<String>,
}

pub fn fixtures() -> Vec<Fixture> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scenes");
    let mut fixtures: Vec<Fixture> = files(&dir, "excalidraw")
        .into_iter()
        .map(|path| Fixture {
            name: name(&path),
            scene: read_json(&path),
            svg: std::fs::read_to_string(path.with_extension("svg")).ok(),
        })
        .collect();
    assert!(!fixtures.is_empty(), "no fixtures in {}", dir.display());

    if let Some(extra) = std::env::var_os("ROUGHDRAFT_EXTRA_FIXTURES") {
        for path in files(Path::new(&extra), "json") {
            let mut memo = read_json(&path);
            let scene = match memo["excalidraw"].take() {
                Value::String(nested) => serde_json::from_str(&nested).unwrap(),
                scene => scene,
            };
            let svg = memo["svg"].as_str().map(str::to_owned);
            fixtures.push(Fixture {
                name: name(&path),
                scene,
                svg,
            });
        }
    }
    fixtures
}

fn files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == extension))
        .collect();
    paths.sort();
    paths
}

fn name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}
